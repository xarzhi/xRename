//! 文件项模型与重命名规则。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Local};

use crate::script::ScriptItem;

/// 条目是怎么被加进列表的。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    /// 直接拖入 / 选择的文件
    File,
    /// 直接拖入 / 选择的文件夹本身
    FolderSelf,
    /// 文件夹里的条目
    FolderChild,
}


#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub ext: String,
    pub stem: String,
    pub dir: String,
    pub size: u64,
    pub is_dir: bool,
    pub origin: Origin,
    /// 创建时间，本地时间 `2024-01-15 18:30:00`；拿不到则为空
    pub create_time: String,
    /// 媒体日期 `20240115_183000`；非图片 / 音频 / 视频为空
    pub media_date: String,
    /// 媒体类型：image / audio / video / document / other / folder
    pub media_type: &'static str,
    /// 脚本算出来的新名字（未落盘）
    pub new_name: String,
    pub error: Option<String>,
    pub renamed: bool,
    pub selected: bool,
}

impl Entry {
    pub fn from_path(path: &Path, origin: Origin) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        let is_dir = meta.is_dir();
        let name = path.file_name()?.to_string_lossy().to_string();
        let (stem, ext) = if is_dir {
            (name.clone(), String::new())
        } else {
            match path.extension() {
                Some(e) => (
                    path.file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| name.clone()),
                    e.to_string_lossy().to_string(),
                ),
                None => (name.clone(), String::new()),
            }
        };
        let media_type = media_type_of(&ext, is_dir);
        let created = meta.created().ok().or_else(|| meta.modified().ok());
        let modified = meta.modified().ok();
        Some(Self {
            dir: path
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            path: path.to_path_buf(),
            name,
            ext,
            stem,
            size: if is_dir { 0 } else { meta.len() },
            is_dir,
            origin,
            create_time: stamp(created, "%Y-%m-%d %H:%M:%S"),
            media_date: if is_media(media_type) {
                stamp(modified, "%Y%m%d_%H%M%S")
            } else {
                String::new()
            },
            media_type,
            new_name: String::new(),
            error: None,
            renamed: false,
            selected: false,
        })
    }

    pub fn script_item(&self) -> ScriptItem {
        ScriptItem {
            name: self.name.clone(),
            ext: self.ext.clone(),
            stem: self.stem.clone(),
            path: self.path.to_string_lossy().to_string(),
            dir: self.dir.clone(),
            create_time: self.create_time.clone(),
            media_date: self.media_date.clone(),
            media_type: self.media_type.to_string(),
            size: self.size,
            is_dir: self.is_dir,
            is_audio: self.media_type == "audio",
            is_document: self.media_type == "document",
            is_image: self.media_type == "image",
            is_video: self.media_type == "video",
        }
    }

    pub fn is_image(&self) -> bool {
        self.media_type == "image"
    }

    pub fn is_audio(&self) -> bool {
        self.media_type == "audio"
    }

    pub fn is_video(&self) -> bool {
        self.media_type == "video"
    }

    pub fn is_document(&self) -> bool {
        self.media_type == "document"
    }

    /// 路径变了以后，把其余字段重新算一遍（撤销改名后要用）。
    pub fn refresh(&mut self) {
        if let Some(fresh) = Entry::from_path(&self.path, self.origin) {
            self.name = fresh.name;
            self.ext = fresh.ext;
            self.stem = fresh.stem;
            self.dir = fresh.dir;
            self.size = fresh.size;
            self.is_dir = fresh.is_dir;
            self.create_time = fresh.create_time;
            self.media_date = fresh.media_date;
            self.media_type = fresh.media_type;
        }
    }

    pub fn has_change(&self) -> bool {
        !self.new_name.is_empty() && self.new_name != self.name
    }

    pub fn display_size(&self) -> String {
        if self.is_dir {
            "—".to_string()
        } else {
            human_size(self.size)
        }
    }
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

// ---------------------------------------------------------------------------
// 媒体类型与时间
// ---------------------------------------------------------------------------

const AUDIO_EXT: &[&str] = &[
    "mp3", "wav", "flac", "aac", "ogg", "oga", "m4a", "wma", "aiff", "aif", "ape", "opus", "mid",
    "midi", "amr", "ac3", "dts",
];

const IMAGE_EXT: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "gif", "bmp", "tif", "tiff", "webp", "heic", "heif",
    "avif", "svg", "ico", "psd", "raw", "dng", "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2",
];

const VIDEO_EXT: &[&str] = &[
    "mp4", "m4v", "mov", "avi", "mkv", "wmv", "flv", "f4v", "webm", "mpg", "mpeg", "m2ts", "mts",
    "ts", "3gp", "3g2", "rmvb", "rm", "vob", "asf", "ogv",
];

const DOCUMENT_EXT: &[&str] = &[
    "txt", "md", "markdown", "pdf", "doc", "docx", "xls", "xlsx", "csv", "tsv", "ppt", "pptx",
    "rtf", "odt", "ods", "odp", "epub", "mobi", "html", "htm", "xml", "json", "yaml", "yml",
    "toml", "ini", "log", "pages", "numbers", "key", "tex", "wps", "et", "dps",
];

/// 按扩展名判断媒体类型。文件夹优先。
pub fn media_type_of(ext: &str, is_dir: bool) -> &'static str {
    if is_dir {
        return "folder";
    }
    let ext = ext.to_ascii_lowercase();
    if ext.is_empty() {
        return "other";
    }
    let hit = |list: &[&str]| list.contains(&ext.as_str());
    if hit(IMAGE_EXT) {
        "image"
    } else if hit(VIDEO_EXT) {
        "video"
    } else if hit(AUDIO_EXT) {
        "audio"
    } else if hit(DOCUMENT_EXT) {
        "document"
    } else {
        "other"
    }
}

pub fn is_media(media_type: &str) -> bool {
    matches!(media_type, "image" | "audio" | "video")
}

fn stamp(time: Option<SystemTime>, fmt: &str) -> String {
    match time {
        Some(t) => DateTime::<Local>::from(t).format(fmt).to_string(),
        None => String::new(),
    }
}

/// 跳过隐藏 / 系统文件，避免批量改名时误伤。
pub fn is_hidden(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') {
            return true;
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN: u32 = 0x2;
        const SYSTEM: u32 = 0x4;
        if let Ok(meta) = std::fs::metadata(path) {
            let attrs = meta.file_attributes();
            return attrs & HIDDEN != 0 || attrs & SYSTEM != 0;
        }
    }
    false
}

/// 读取文件夹里的文件。
///
/// `recursive` 为真时把各层子文件夹里的文件也一并收进来；为假时只看这一层。
/// 只收文件，不收文件夹本身 —— 要改文件夹的名字，把文件夹单独拖进来选「重命名文件夹本身」。
pub fn collect_children(dir: &Path, recursive: bool) -> Vec<Entry> {
    let mut out = Vec::new();
    walk_files(dir, recursive, &mut out);
    out
}

/// 深度优先：先这一层的文件（按自然序），再挨个进子文件夹，同样的规矩。
fn walk_files(dir: &Path, recursive: bool, out: &mut Vec<Entry>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<Entry> = Vec::new();
    let mut subdirs: Vec<(String, PathBuf)> = Vec::new();

    for item in read.flatten() {
        let path = item.path();
        if is_hidden(&path) {
            continue;
        }
        let Ok(file_type) = item.file_type() else {
            continue;
        };
        // 不跟符号链接 / 目录联接，免得绕成圈
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if recursive {
                let name = item.file_name().to_string_lossy().to_string();
                subdirs.push((name, path));
            }
            continue;
        }
        if let Some(entry) = Entry::from_path(&path, Origin::FolderChild) {
            files.push(entry);
        }
    }

    files.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    out.append(&mut files);

    subdirs.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    for (_, path) in subdirs {
        walk_files(&path, true, out);
    }
}

/// 自然排序：`a2` 排在 `a10` 前面。
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(ac), Some(bc)) => {
                if ac.is_ascii_digit() && bc.is_ascii_digit() {
                    let an = take_digits(&mut ai);
                    let bn = take_digits(&mut bi);
                    let ord = an.cmp(&bn);
                    if ord != std::cmp::Ordering::Equal {
                        return ord;
                    }
                } else {
                    ai.next();
                    bi.next();
                    let ord = ac.to_lowercase().cmp(bc.to_lowercase());
                    if ord != std::cmp::Ordering::Equal {
                        return ord;
                    }
                }
            }
        }
    }
}

fn take_digits(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u128 {
    let mut s = String::new();
    while let Some(c) = it.peek().copied() {
        if c.is_ascii_digit() {
            s.push(c);
            it.next();
        } else {
            break;
        }
    }
    s.parse().unwrap_or(0)
}

const INVALID_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("新文件名为空".to_string());
    }
    if let Some(c) = name.chars().find(|c| INVALID_CHARS.contains(c)) {
        return Err(format!("包含非法字符 `{c}`"));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err("结尾不能是点或空格".to_string());
    }
    if name.chars().any(|c| (c as u32) < 0x20) {
        return Err("包含控制字符".to_string());
    }
    let base = name.split('.').next().unwrap_or(name).to_uppercase();
    if RESERVED.contains(&base.as_str()) {
        return Err(format!("`{base}` 是系统保留名"));
    }
    if name.chars().count() > 200 {
        return Err("文件名过长".to_string());
    }
    Ok(())
}

/// 批量改名结果
#[derive(Default, Debug)]
pub struct CommitReport {
    pub renamed: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// 真正落盘改名。分两步走，避免 A→B、B→C 这种链式改名互相覆盖。
pub fn commit(entries: &mut [Entry]) -> CommitReport {
    let mut report = CommitReport::default();

    // 只处理「有变化且无错误」的行
    let targets: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.error.is_none() && e.has_change() && !e.renamed)
        .map(|(i, _)| i)
        .collect();

    report.skipped = entries.len() - targets.len();

    // 第一步：改成临时名
    let mut staged: Vec<(usize, PathBuf)> = Vec::new();
    for &i in &targets {
        let entry = &entries[i];
        let dir = Path::new(&entry.dir);
        let tmp = dir.join(format!(".xrename_tmp_{:08x}", abs_hash(&entry.path)));
        match std::fs::rename(&entry.path, &tmp) {
            Ok(_) => staged.push((i, tmp)),
            Err(err) => {
                entries[i].error = Some(format!("改名失败: {}", friendly_io(&err)));
                report.failed += 1;
            }
        }
    }

    // 第二步：临时名 -> 目标名
    for (i, tmp) in staged {
        let entry = &entries[i];
        let target = Path::new(&entry.dir).join(&entry.new_name);
        if target.exists() {
            // 目标已存在，回滚
            let _ = std::fs::rename(&tmp, &entries[i].path);
            entries[i].error = Some("目标已存在".to_string());
            report.failed += 1;
            continue;
        }
        match std::fs::rename(&tmp, &target) {
            Ok(_) => {
                entries[i].path = target;
                entries[i].name = entries[i].new_name.clone();
                entries[i].dir = entries[i]
                    .path
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                if entries[i].is_dir {
                    entries[i].stem = entries[i].name.clone();
                } else {
                    let p = entries[i].path.clone();
                    entries[i].stem = p
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| entries[i].name.clone());
                    entries[i].ext = p
                        .extension()
                        .map(|e| e.to_string_lossy().to_string())
                        .unwrap_or_default();
                }
                entries[i].renamed = true;
                report.renamed += 1;
            }
            Err(err) => {
                let _ = std::fs::rename(&tmp, &entries[i].path);
                entries[i].error = Some(format!("改名失败: {}", friendly_io(&err)));
                report.failed += 1;
            }
        }
    }

    report
}

fn abs_hash(path: &Path) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.to_string_lossy().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

pub fn friendly_io(err: &std::io::Error) -> String {
    match err.kind() {
        std::io::ErrorKind::PermissionDenied => "没有权限（文件可能被占用或只读）".to_string(),
        std::io::ErrorKind::NotFound => "文件不存在".to_string(),
        _ => err.to_string(),
    }
}

/// 跑一遍脚本，把结果写进 `new_name` / `error`，**不动磁盘**。
pub fn apply_script(
    entries: &mut [Entry],
    script: &crate::script::CompiledScript,
    keep_ext: bool,
) {
    for (index, entry) in entries.iter_mut().enumerate() {
        let item = entry.script_item();
        match script.run(&item, index) {
            Ok(raw) => {
                let mut name = raw.trim().to_string();
                if name.is_empty() {
                    entry.new_name.clear();
                    entry.error = Some("脚本返回空字符串".to_string());
                    continue;
                }
                // 用户没写扩展名时自动补上，避免一不小心把后缀抹掉
                if keep_ext && !entry.ext.is_empty() && !name.contains('.') {
                    name = format!("{name}.{}", entry.ext);
                }
                if name != entry.name {
                    if let Err(err) = validate_name(&name) {
                        entry.new_name = name;
                        entry.error = Some(err);
                        continue;
                    }
                }
                entry.new_name = name;
                entry.error = None;
            }
            Err(err) => {
                entry.new_name.clear();
                entry.error = Some(err);
            }
        }
    }
    mark_duplicates(entries);
}

/// 同一目录下算出来的新名字不能撞车。
fn mark_duplicates(entries: &mut [Entry]) {
    let mut seen: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();
    for (i, entry) in entries.iter_mut().enumerate() {
        if entry.error.is_some() || entry.new_name.is_empty() || !entry.has_change() {
            continue;
        }
        let key = (entry.dir.to_lowercase(), entry.new_name.to_lowercase());
        match seen.get(&key) {
            Some(first) => {
                entry.error = Some(format!("与第 {} 项的新名字重复", first + 1));
            }
            None => {
                seen.insert(key, i);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 自定义规则（模板拼接）
// ---------------------------------------------------------------------------

/// 规则里可以用的变量：`(写法, 说明, 示例)`。
pub const RULE_VARS: &[(&str, &str, &str)] = &[
    ("{name}", "文件名，含扩展名", "photo.jpg"),
    ("{ext}", "扩展名，不含点", "jpg"),
    ("{stem}", "不含扩展名的文件名", "photo"),
    ("{path}", "完整路径", "C:\\pics\\photo.jpg"),
    ("{dir}", "所在文件夹", "C:\\pics"),
    ("{size}", "字节数，文件夹为 0", "20480"),
    ("{is_dir}", "是否是文件夹", "false"),
    ("{index}", "当前下标，从 0 开始", "0"),
    ("{seq}", "当前序号，从 1 开始", "1"),
    ("{createTime}", "创建时间", "2024-01-15 18:30:00"),
    ("{mediaDate}", "媒体日期，非媒体为空", "20240115_183000"),
    ("{mediaType}", "媒体类型", "image"),
    ("{isImage}", "是否图片", "true"),
    ("{isAudio}", "是否音频", "false"),
    ("{isVideo}", "是否视频", "false"),
    ("{isDocument}", "是否文档", "false"),
];

fn rule_value(entry: &Entry, index: usize, key: &str) -> Option<String> {
    let key = key.strip_prefix("item.").unwrap_or(key);
    Some(match key {
        "name" => entry.name.clone(),
        "ext" => entry.ext.clone(),
        "stem" => entry.stem.clone(),
        "path" => entry.path.to_string_lossy().to_string(),
        "dir" => entry.dir.clone(),
        "size" => entry.size.to_string(),
        "is_dir" | "bool" => entry.is_dir.to_string(),
        "index" => index.to_string(),
        "seq" | "num" => (index + 1).to_string(),
        "createTime" | "create_time" => entry.create_time.clone(),
        "mediaDate" | "media_date" => entry.media_date.clone(),
        "mediaType" | "media_type" => entry.media_type.to_string(),
        "isImage" | "is_image" => entry.is_image().to_string(),
        "isAudio" | "is_audio" => entry.is_audio().to_string(),
        "isVideo" | "is_video" => entry.is_video().to_string(),
        "isDocument" | "is_document" => entry.is_document().to_string(),
        _ => return None,
    })
}

fn pad_value(value: &str, spec: &str) -> String {
    let spec = spec.trim();
    if spec.is_empty() {
        return value.to_string();
    }
    // `{seq:3}` -> 补零到 3 位；`{seq:03}` 也接受
    if let Ok(width) = spec.trim_start_matches('0').parse::<usize>() {
        if width > 0 && spec.chars().all(|c| c.is_ascii_digit()) {
            let cur = value.chars().count();
            if cur < width {
                return format!("{}{}", "0".repeat(width - cur), value);
            }
        }
    }
    value.to_string()
}

/// 扫描一遍 `{变量}`，检查有没有写错的变量名。
fn check_rule(pattern: &str) -> Result<(), String> {
    let cs: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        match cs[i] {
            '{' if cs.get(i + 1) == Some(&'{') => i += 2,
            '}' if cs.get(i + 1) == Some(&'}') => i += 2,
            '{' => {
                let Some(offset) = cs[i + 1..].iter().position(|c| *c == '}') else {
                    return Err("规则里有未闭合的 `{`".to_string());
                };
                let inner: String = cs[i + 1..i + 1 + offset].iter().collect();
                let key = inner.split(':').next().unwrap_or("").trim();
                let known = RULE_VARS.iter().any(|(v, _, _)| {
                    *v == format!("{{{key}}}") || *v == format!("{{item.{key}}}")
                });
                if !known {
                    return Err(format!("规则里有未知变量 `{{{key}}}`"));
                }
                i = i + offset + 2;
            }
            _ => i += 1,
        }
    }
    Ok(())
}

fn expand_rule(pattern: &str, entry: &Entry, index: usize) -> String {
    let cs: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        match cs[i] {
            '{' if cs.get(i + 1) == Some(&'{') => {
                out.push('{');
                i += 2;
            }
            '}' if cs.get(i + 1) == Some(&'}') => {
                out.push('}');
                i += 2;
            }
            '{' => match cs[i + 1..].iter().position(|c| *c == '}') {
                Some(offset) => {
                    let inner: String = cs[i + 1..i + 1 + offset].iter().collect();
                    let (key, spec) = match inner.split_once(':') {
                        Some((k, s)) => (k.trim(), s),
                        None => (inner.trim(), ""),
                    };
                    if let Some(value) = rule_value(entry, index, key) {
                        out.push_str(&pad_value(&value, spec));
                    }
                    i = i + offset + 2;
                }
                None => {
                    out.push('{');
                    i += 1;
                }
            },
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// 按用户写的规则算新名字，**不动磁盘**。
pub fn apply_rule(entries: &mut [Entry], pattern: &str) -> Result<(), String> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Err("规则为空".to_string());
    }
    check_rule(pattern)?;

    for (index, entry) in entries.iter_mut().enumerate() {
        let name = expand_rule(pattern, entry, index).trim().to_string();
        if name.is_empty() {
            entry.new_name.clear();
            entry.error = Some("规则算出来是空名字".to_string());
            continue;
        }
        if name != entry.name {
            if let Err(err) = validate_name(&name) {
                entry.new_name = name;
                entry.error = Some(err);
                continue;
            }
        }
        entry.new_name = name;
        entry.error = None;
    }
    mark_duplicates(entries);
    Ok(())
}

// ---------------------------------------------------------------------------
// 撤销
// ---------------------------------------------------------------------------

#[derive(Default, Debug)]
pub struct UndoReport {
    /// 成功撤回的记录下标
    pub undone: Vec<usize>,
    pub failed: usize,
}

/// 把已经改过的名字改回去（两阶段，避免互相覆盖）。`records` 是 `(改名前, 改名后)`。
pub fn undo(records: &[(PathBuf, PathBuf)]) -> UndoReport {
    let mut report = UndoReport::default();
    let mut staged: Vec<(usize, PathBuf, PathBuf)> = Vec::new();

    for (i, (_, new)) in records.iter().enumerate() {
        if !new.exists() {
            report.failed += 1;
            continue;
        }
        let dir = new.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let tmp = dir.join(format!(".xrename_undo_{:04}_{:08x}", i, abs_hash(new)));
        match std::fs::rename(new, &tmp) {
            Ok(_) => staged.push((i, tmp, records[i].0.clone())),
            Err(_) => report.failed += 1,
        }
    }

    for (i, tmp, old) in staged {
        if old.exists() {
            // 原名字被占用了，先退回改名后的状态
            let _ = std::fs::rename(&tmp, &records[i].1);
            report.failed += 1;
            continue;
        }
        match std::fs::rename(&tmp, &old) {
            Ok(_) => report.undone.push(i),
            Err(_) => {
                let _ = std::fs::rename(&tmp, &records[i].1);
                report.failed += 1;
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{Lang, ScriptCache};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "xrename-test-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn validate_rules() {
        assert!(validate_name("ok.txt").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("a/b.txt").is_err());
        assert!(validate_name("trail.").is_err());
        assert!(validate_name("CON.txt").is_err());
    }

    #[test]
    fn rename_with_script_end_to_end() {
        let dir = temp_dir("rename");
        std::fs::write(dir.join("alpha.txt"), b"1").unwrap();
        std::fs::write(dir.join("beta.txt"), b"2").unwrap();

        let mut entries = collect_children(&dir, false);
        assert_eq!(entries.len(), 2);

        let mut cache = ScriptCache::new();
        let script = cache
            .build(Lang::Js, "return item.stem.toUpperCase() + \".\" + item.ext")
            .expect("compile");
        apply_script(&mut entries, &script, true);

        let names: Vec<&str> = entries.iter().map(|e| e.new_name.as_str()).collect();
        assert_eq!(names, vec!["ALPHA.txt", "BETA.txt"]);
        assert!(entries.iter().all(|e| e.error.is_none()));

        let report = commit(&mut entries);
        assert_eq!(report.renamed, 2, "report: {report:?}");
        assert!(dir.join("ALPHA.txt").exists());
        assert!(dir.join("BETA.txt").exists());
        let count = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(count, 2, "不应残留旧名字的文件");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_names_are_flagged() {
        let dir = temp_dir("dup");
        std::fs::write(dir.join("one.txt"), b"1").unwrap();
        std::fs::write(dir.join("two.txt"), b"2").unwrap();

        let mut entries = collect_children(&dir, false);
        let mut cache = ScriptCache::new();
        let script = cache
            .build(Lang::Rust, "\"same.txt\".to_string()")
            .expect("compile");
        apply_script(&mut entries, &script, true);

        assert_eq!(entries.iter().filter(|e| e.error.is_some()).count(), 1);
        let report = commit(&mut entries);
        assert_eq!(report.renamed, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_children_respects_recursive_flag() {
        let dir = temp_dir("collect");
        std::fs::write(dir.join("top.txt"), b"1").unwrap();
        let sub = dir.join("sub");
        std::fs::create_dir_all(sub.join("deeper")).unwrap();
        std::fs::write(sub.join("mid.txt"), b"2").unwrap();
        std::fs::write(sub.join("deeper").join("low.txt"), b"3").unwrap();

        // 不勾选：只收这一层的文件，子文件夹整个跳过
        let flat = collect_children(&dir, false);
        let names: Vec<&str> = flat.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["top.txt"], "flat: {names:?}");

        // 勾选：把各层子文件夹里的文件都收进来
        let deep = collect_children(&dir, true);
        let names: Vec<&str> = deep.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["top.txt", "mid.txt", "low.txt"], "deep: {names:?}");
        // 文件夹本身不算条目
        assert!(deep.iter().all(|e| !e.is_dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rule_expands_vars() {
        let dir = temp_dir("rule");
        std::fs::write(dir.join("a.txt"), b"1").unwrap();
        std::fs::write(dir.join("b.txt"), b"2").unwrap();

        let mut entries = collect_children(&dir, false);
        apply_rule(&mut entries, "{seq:2}_{stem}.{ext}").expect("rule");
        let names: Vec<&str> = entries.iter().map(|e| e.new_name.as_str()).collect();
        assert_eq!(names, vec!["01_a.txt", "02_b.txt"]);
        assert!(entries.iter().all(|e| e.error.is_none()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rule_rejects_unknown_var() {
        let dir = temp_dir("rule-bad");
        std::fs::write(dir.join("a.txt"), b"1").unwrap();

        let mut entries = collect_children(&dir, false);
        let err = apply_rule(&mut entries, "{nope}.txt").err().unwrap_or_default();
        assert!(err.contains("未知变量"), "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_restores_names() {
        let dir = temp_dir("undo");
        std::fs::write(dir.join("a.txt"), b"1").unwrap();
        std::fs::write(dir.join("b.txt"), b"2").unwrap();

        let mut entries = collect_children(&dir, false);
        let before: Vec<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();

        apply_rule(&mut entries, "{seq:2}_{stem}.{ext}").unwrap();
        assert_eq!(commit(&mut entries).renamed, 2);
        assert!(dir.join("01_a.txt").exists());

        let records: Vec<(PathBuf, PathBuf)> = entries
            .iter()
            .enumerate()
            .filter(|(i, e)| e.path != before[*i])
            .map(|(i, e)| (before[i].clone(), e.path.clone()))
            .collect();
        let rep = undo(&records);
        assert_eq!(rep.undone.len(), 2, "{rep:?}");

        assert!(dir.join("a.txt").exists());
        assert!(dir.join("b.txt").exists());
        assert!(!dir.join("01_a.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
