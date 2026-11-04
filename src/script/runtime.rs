//! 脚本运行时：把转译后的 Rust 代码用 `rustc` 编译成动态库，加载后逐个文件调用。
//!
//! 这样做的好处是「脚本最终就是 Rust 在跑」，速度和控制力都与原生一致；
//! 代价是第一次应用某个脚本要花几百毫秒编译，编译结果会按内容哈希缓存。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use libloading::Library;

use super::transpile::{Lang, to_rust};

/// 传给脚本的 `item`。
pub struct ScriptItem {
    pub name: String,
    pub ext: String,
    pub stem: String,
    pub path: String,
    pub dir: String,
    pub create_time: String,
    pub media_date: String,
    pub media_type: String,
    pub size: u64,
    pub is_dir: bool,
    pub is_audio: bool,
    pub is_document: bool,
    pub is_image: bool,
    pub is_video: bool,
}

type TransformFn = unsafe extern "C" fn(
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    *mut u8,
    usize,
) -> i64;

pub struct CompiledScript {
    _lib: Library,
    func: TransformFn,
    /// 转译出来的 Rust 代码，界面上可以看「转换结果」。
    pub rust_code: String,
}

impl CompiledScript {
    pub fn run(&self, item: &ScriptItem, index: usize) -> Result<String, String> {
        let mut out = vec![0u8; 64 * 1024];
        let n = unsafe {
            (self.func)(
                item.name.as_ptr(),
                item.name.len(),
                item.ext.as_ptr(),
                item.ext.len(),
                item.stem.as_ptr(),
                item.stem.len(),
                item.path.as_ptr(),
                item.path.len(),
                item.dir.as_ptr(),
                item.dir.len(),
                item.create_time.as_ptr(),
                item.create_time.len(),
                item.media_date.as_ptr(),
                item.media_date.len(),
                item.media_type.as_ptr(),
                item.media_type.len(),
                item.size,
                item.is_dir as u64,
                index as u64,
                item.is_audio as u64,
                item.is_document as u64,
                item.is_image as u64,
                item.is_video as u64,
                out.as_mut_ptr(),
                out.len(),
            )
        };
        if n >= 0 {
            out.truncate(n as usize);
            String::from_utf8(out).map_err(|_| "脚本返回的不是合法 UTF-8 字符串".to_string())
        } else {
            Err(match n {
                -2 => "脚本返回值过长（超过 64KB）".to_string(),
                -3 => "脚本运行 panic，请检查取值是否越界".to_string(),
                other => format!("脚本运行失败（错误码 {other}）"),
            })
        }
    }
}

pub struct ScriptCache {
    dir: PathBuf,
    cache: HashMap<u64, Result<Arc<CompiledScript>, String>>,
}

impl Default for ScriptCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptCache {
    pub fn new() -> Self {
        let dir = std::env::temp_dir().join("xrename").join("scripts");
        let _ = std::fs::create_dir_all(&dir);
        Self {
            dir,
            cache: HashMap::new(),
        }
    }

    /// 转译 + 编译（带缓存）。同一个脚本内容只会编译一次。
    pub fn build(&mut self, lang: Lang, body: &str) -> Result<Arc<CompiledScript>, String> {
        let rust_body = to_rust(lang, body)?;
        let source = render_source(&rust_body);
        let hash = fnv1a(&source);
        if let Some(hit) = self.cache.get(&hash) {
            return hit.clone();
        }
        let built = self.compile_source(&source, hash);
        self.cache.insert(hash, built.clone());
        built
    }

    fn compile_source(
        &self,
        source: &str,
        hash: u64,
    ) -> Result<Arc<CompiledScript>, String> {
        let stem = format!("xs_{hash:016x}");
        let rs_path = self.dir.join(format!("{stem}.rs"));
        let dll_path = self.dir.join(format!("{stem}.dll"));

        if !dll_path.exists() {
            std::fs::write(&rs_path, source)
                .map_err(|e| format!("无法写入临时脚本文件: {e}"))?;
            let output = run_rustc(&rs_path, &dll_path, &stem)?;
            if !output.status.success() {
                let mut err = String::from_utf8_lossy(&output.stderr).to_string();
                if err.trim().is_empty() {
                    err = String::from_utf8_lossy(&output.stdout).to_string();
                }
                return Err(short_error(&err));
            }
        }

        let lib = unsafe { Library::new(&dll_path) }
            .map_err(|e| format!("无法加载编译产物: {e}"))?;
        let func: TransformFn = unsafe {
            let sym = lib
                .get::<TransformFn>(b"xrename_transform\0")
                .map_err(|e| format!("编译产物缺少入口函数: {e}"))?;
            *sym
        };

        Ok(Arc::new(CompiledScript {
            _lib: lib,
            func,
            rust_code: source.to_string(),
        }))
    }
}

fn run_rustc(rs: &Path, dll: &Path, crate_name: &str) -> Result<std::process::Output, String> {
    let mut cmd = Command::new("rustc");
    cmd.arg("--crate-type")
        .arg("cdylib")
        .arg("--edition")
        .arg("2021")
        .arg("-C")
        .arg("opt-level=2")
        .arg("-A")
        .arg("warnings")
        .arg("--crate-name")
        .arg(crate_name)
        .arg("-o")
        .arg(dll)
        .arg(rs);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    cmd.output().map_err(|e| {
        format!(
            "无法启动 rustc（{e}）。生成脚本需要本机已安装 Rust 工具链，请确认 rustc 在 PATH 中。"
        )
    })
}

fn short_error(stderr: &str) -> String {
    // rustc 的报错第一段通常最有价值，去掉绝对路径里的临时目录噪音
    let mut lines: Vec<&str> = stderr
        .lines()
        .map(|l| l.trim_end())
        .filter(|l| !l.trim().is_empty())
        .collect();
    if lines.len() > 8 {
        lines.truncate(8);
    }
    lines.join("\n")
}

fn fnv1a(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

// ---------------------------------------------------------------------------
// 生成 Rust 源码
// ---------------------------------------------------------------------------

fn render_source(body: &str) -> String {
    format!(
        r##"#![allow(dead_code, unused_variables, unused_mut, unused_parens, non_snake_case)]

pub struct Item {{
    pub name: String,
    pub ext: String,
    pub stem: String,
    pub path: String,
    pub dir: String,
    pub createTime: String,
    pub mediaDate: String,
    pub mediaType: String,
    pub size: u64,
    pub is_dir: bool,
    pub isAudio: bool,
    pub isDocument: bool,
    pub isImage: bool,
    pub isVideo: bool,
}}

fn to_upper(s: impl AsRef<str>) -> String {{ s.as_ref().to_uppercase() }}
fn to_lower(s: impl AsRef<str>) -> String {{ s.as_ref().to_lowercase() }}
fn trim_str(s: impl AsRef<str>) -> String {{ s.as_ref().trim().to_string() }}
fn replace_all(s: impl AsRef<str>, from: &str, to: &str) -> String {{ s.as_ref().replace(from, to) }}
fn split(s: impl AsRef<str>, sep: &str) -> Vec<String> {{
    if sep.is_empty() {{
        s.as_ref().chars().map(|c| c.to_string()).collect()
    }} else {{
        s.as_ref().split(sep).map(|x| x.to_string()).collect()
    }}
}}
fn join(parts: &[String], sep: &str) -> String {{ parts.join(sep) }}
fn len_str(s: impl AsRef<str>) -> usize {{ s.as_ref().chars().count() }}
fn sub(s: impl AsRef<str>, start: i64, end: i64) -> String {{
    let cs: Vec<char> = s.as_ref().chars().collect();
    let n = cs.len() as i64;
    let raw_a = if start < 0 {{ n + start }} else {{ start }};
    let raw_b = if end < 0 {{ n + end }} else {{ end }};
    let a = raw_a.clamp(0, n);
    let b = raw_b.clamp(0, n);
    if a >= b {{ return String::new(); }}
    cs[a as usize..b as usize].iter().collect()
}}
fn sub_from(s: impl AsRef<str>, start: i64) -> String {{ sub(s, start, i64::MAX) }}
fn sub_len(s: impl AsRef<str>, start: i64, len: i64) -> String {{ sub(s, start, start + len) }}
fn pad_start(s: impl AsRef<str>, width: usize, fill: &str) -> String {{
    let text = s.as_ref();
    let cur = text.chars().count();
    if cur >= width || fill.is_empty() {{ return text.to_string(); }}
    let pad: String = fill.chars().cycle().take(width - cur).collect();
    format!("{{}}{{}}", pad, text)
}}
fn starts_with_h(s: impl AsRef<str>, p: &str) -> bool {{ s.as_ref().starts_with(p) }}
fn ends_with_h(s: impl AsRef<str>, p: &str) -> bool {{ s.as_ref().ends_with(p) }}
fn contains_h(s: impl AsRef<str>, p: &str) -> bool {{ s.as_ref().contains(p) }}
fn find_pos(s: impl AsRef<str>, p: &str) -> i64 {{
    match s.as_ref().find(p) {{ Some(i) => i as i64, None => -1 }}
}}
fn push_str_h(mut s: String, extra: impl AsRef<str>) -> String {{ s.push_str(extra.as_ref()); s }}
fn to_int(s: impl AsRef<str>) -> i64 {{ s.as_ref().trim().parse::<i64>().unwrap_or(0) }}

fn xsl(ptr: *const u8, len: usize) -> String {{
    if ptr.is_null() || len == 0 {{ return String::new(); }}
    let bytes = unsafe {{ std::slice::from_raw_parts(ptr, len) }};
    String::from_utf8_lossy(bytes).into_owned()
}}

fn xwrite(s: &str, out: *mut u8, cap: usize) -> i64 {{
    let bytes = s.as_bytes();
    if bytes.len() > cap {{ return -2; }}
    if !out.is_null() && !bytes.is_empty() {{
        unsafe {{ std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len()) }};
    }}
    bytes.len() as i64
}}

#[no_mangle]
pub extern "C" fn xrename_transform(
    name_p: *const u8, name_n: usize,
    ext_p: *const u8, ext_n: usize,
    stem_p: *const u8, stem_n: usize,
    path_p: *const u8, path_n: usize,
    dir_p: *const u8, dir_n: usize,
    create_time_p: *const u8, create_time_n: usize,
    media_date_p: *const u8, media_date_n: usize,
    media_type_p: *const u8, media_type_n: usize,
    size: u64,
    is_dir: u64,
    index: u64,
    is_audio: u64,
    is_document: u64,
    is_image: u64,
    is_video: u64,
    out: *mut u8,
    out_cap: usize,
) -> i64 {{
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> String {{
        let item = Item {{
            name: xsl(name_p, name_n),
            ext: xsl(ext_p, ext_n),
            stem: xsl(stem_p, stem_n),
            path: xsl(path_p, path_n),
            dir: xsl(dir_p, dir_n),
            createTime: xsl(create_time_p, create_time_n),
            mediaDate: xsl(media_date_p, media_date_n),
            mediaType: xsl(media_type_p, media_type_n),
            size,
            is_dir: is_dir != 0,
            isAudio: is_audio != 0,
            isDocument: is_document != 0,
            isImage: is_image != 0,
            isVideo: is_video != 0,
        }};
        let index: usize = index as usize;
{body}
    }}));
    match result {{
        Ok(s) => xwrite(&s, out, out_cap),
        Err(_) => -3,
    }}
}}
"##,
        body = indent(body, 8)
    )
}

fn indent(text: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    text.lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("{pad}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> ScriptItem {
        ScriptItem {
            name: "photo.JPG".to_string(),
            ext: "JPG".to_string(),
            stem: "photo".to_string(),
            path: "C:////pics////photo.JPG".to_string(),
            dir: "C:////pics".to_string(),
            create_time: "2024-01-15 18:30:00".to_string(),
            media_date: "20240115_183000".to_string(),
            media_type: "image".to_string(),
            size: 1024,
            is_dir: false,
            is_audio: false,
            is_document: false,
            is_image: true,
            is_video: false,
        }
    }

    #[test]
    fn js_end_to_end() {
        let mut cache = ScriptCache::new();
        let script = cache
            .build(Lang::Js, "return `${item.stem}_${index + 1}.${item.ext}`;")
            .expect("compile");
        assert_eq!(script.run(&item(), 4).unwrap(), "photo_5.JPG");
    }

    #[test]
    fn python_end_to_end() {
        let mut cache = ScriptCache::new();
        let body = r#"if item.is_dir:
    return "D_" + item.name
else:
    return f"{item.stem}_{index:03d}.{item.ext}""#;
        let script = cache.build(Lang::Python, body).expect("compile");
        assert_eq!(script.run(&item(), 7).unwrap(), "photo_007.JPG");
    }

    #[test]
    fn every_language_template_compiles() {
        let mut cache = ScriptCache::new();
        for lang in Lang::all() {
            let script = cache
                .build(lang, lang.template())
                .unwrap_or_else(|e| panic!("{} 模板编译失败: {e}", lang.label()));
            let out = script
                .run(&item(), 6)
                .unwrap_or_else(|e| panic!("{} 模板运行失败: {e}", lang.label()));
            assert!(!out.trim().is_empty(), "{} 输出为空", lang.label());
            assert!(!out.contains('\n'), "{} 输出含换行: {out}", lang.label());
        }
    }

    #[test]
    fn media_fields_are_visible_to_scripts() {
        let mut cache = ScriptCache::new();
        let script = cache
            .build(
                Lang::Js,
                "return `${item.mediaDate}_${item.mediaType}_${item.createTime}`;",
            )
            .expect("compile");
        assert_eq!(
            script.run(&item(), 0).unwrap(),
            "20240115_183000_image_2024-01-15 18:30:00"
        );
    }

    #[test]
    fn bad_script_reports_error() {
        let mut cache = ScriptCache::new();
        let err = match cache.build(Lang::Js, "return item.nope();") {
            Ok(_) => String::new(),
            Err(e) => e,
        };
        assert!(!err.trim().is_empty(), "expected a compile error");
    }
}
