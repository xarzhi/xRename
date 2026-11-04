#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod columns;
mod complete;
mod highlight;
mod lang_select;
mod model;
mod script;
mod table;

use std::sync::Arc;

use app::RenameApp;
use gpui_kit::*;

/// 程序版本：build.rs 从 Cargo.toml 抄过来，打包脚本改完 Cargo.toml 就跟着变。
pub const VERSION: &str = env!("XRENAME_VERSION");

fn main() {
    // 任务栏 / 资源管理器里的图标是 build.rs 编进 PE 资源段的；
    // 这里的 icon 只对 X11 之类的平台有效，Windows 会忽略。
    let icon = image::load_from_memory(include_bytes!("../assets/logo-128.png"))
        .ok()
        .map(|image| Arc::new(image.to_rgba8()));

    // 注册图标资源，组件的勾选、箭头等图标才有内容可画
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);

            let bounds = Bounds {
                origin: point(px(160.0), px(90.0)),
                size: size(px(1440.0), px(900.0)),
            };
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(format!("xRename v{VERSION} — 批量重命名").into()),
                    appears_transparent: false,
                    traffic_light_position: None,
                }),
                icon,
                ..Default::default()
            };

            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| RenameApp::new(window, cx))
            })
            .expect("无法创建窗口");
        });
}
