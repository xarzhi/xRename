//! 把图标和版本信息编进 Windows exe。
//!
//! exe 的图标不是运行时画上去的，是编译期写进 PE 资源段的，
//! 所以任务栏、Alt+Tab、资源管理器里看到的都是这里塞的 `assets/logo.ico`。
//! 版本号直接取 `Cargo.toml` 的 `[package].version`，打包脚本改完那行就来编。

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/logo.ico");
    println!("cargo:rerun-if-changed=build.rs");

    // 让程序里能用 env!("XRENAME_VERSION")，标题栏就能显示真实版本
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    println!("cargo:rustc-env=XRENAME_VERSION={version}");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let (major, minor, patch, build) = split_version(&version);

    let mut res = winresource::WindowsResource::new();
    res.set_icon_with_id("assets/logo.ico", "1");
    res.set("ProductName", "xRename");
    res.set("FileDescription", "xRename — 批量重命名");
    res.set("LegalCopyright", "xRename");
    res.set("OriginalFilename", "xRename.exe");
    res.set("InternalName", "xRename");
    // 属性面板里「文件版本 / 产品版本」两个字符串，以及内部比较用的数字版本
    res.set("FileVersion", &version);
    res.set("ProductVersion", &version);
    res.set_version_info(winresource::VersionInfo::FILEVERSION, pack(major, minor, patch, build));
    res.set_version_info(
        winresource::VersionInfo::PRODUCTVERSION,
        pack(major, minor, patch, build),
    );
    // 自己定位 rc.exe：默认实现要跑 reg.exe 查注册表，换个受限环境就找不到
    if let Some(dir) = find_rc_dir() {
        res.set_toolkit_path(&dir.to_string_lossy());
    }

    if let Err(err) = res.compile() {
        // 实在找不到资源编译器时不让构建失败，只是 exe 没图标没版本号
        println!("cargo:warning=写入 exe 图标/版本信息失败：{err}");
        println!(
            "cargo:warning=可以手动指定资源编译器所在目录：set RC_PATH=<...\\x64\\rc.exe>"
        );
    }
}

/// 找装 `rc.exe` 的目录。优先级：RC_PATH → PATH 里的 rc.exe → 各磁盘的
/// `Windows Kits\10\bin\<版本>\<架构>` → Windows Kits\8.1。
fn find_rc_dir() -> Option<PathBuf> {
    if let Some(rc) = std::env::var_os("RC_PATH") {
        let rc = PathBuf::from(rc);
        if rc.is_file() {
            return rc.parent().map(Path::to_path_buf);
        }
    }

    if let Ok(out) = Command::new("where.exe").arg("rc.exe").output() {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let path = PathBuf::from(line.trim());
                if path.is_file() {
                    return path.parent().map(Path::to_path_buf);
                }
            }
        }
    }

    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86") => "x86",
        _ => "x64",
    };

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(sdk) = std::env::var_os("WindowsSdkDir") {
        roots.push(PathBuf::from(sdk).join("bin"));
    }
    for drive in b'C'..=b'Z' {
        let base = format!("{}:\\Windows Kits", drive as char);
        roots.push(Path::new(&base).join("10").join("bin"));
        roots.push(Path::new(&base).join("8.1").join("bin"));
    }

    let mut best: Option<(Vec<u32>, PathBuf)> = None;
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let version_dir = entry.path();
            let candidates = [version_dir.join(arch), version_dir.join("x64")];
            for dir in candidates {
                if dir.join("rc.exe").is_file() {
                    let key = version_key(&version_dir);
                    if best.as_ref().is_none_or(|(k, _)| key > *k) {
                        best = Some((key, dir));
                    }
                    break;
                }
            }
        }
    }
    best.map(|(_, dir)| dir)
}

/// `10.0.22621.0` → [10, 0, 22621, 0]，用来挑最新的 SDK
fn version_key(dir: &Path) -> Vec<u32> {
    dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// `1.2.3` / `1.2.3-beta.1` → (1, 2, 3, 0)
fn split_version(version: &str) -> (u16, u16, u16, u16) {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let mut parts = core.split('.').map(|p| p.parse::<u16>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// Windows 版本资源是四个 16 位字段拼起来的一个数
fn pack(major: u16, minor: u16, patch: u16, build: u16) -> u64 {
    (u64::from(major) << 48) | (u64::from(minor) << 32) | (u64::from(patch) << 16) | u64::from(build)
}
