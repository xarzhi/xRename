//! 脚本转译层。
//!
//! 用户可以在界面上用 JS / C++ / Python / Java / Rust 写一段函数体，
//! 这里把它转换成 Rust 代码，再交给 `runtime` 编译成动态库执行。
//!
//! 只支持「函数体」：一段返回字符串的语句，两个隐式参数 `item` 与 `index`。
//! 支持的语法范围见 `Lang::cheat_sheet()`。

mod runtime;
mod transpile;

pub use runtime::{CompiledScript, ScriptCache, ScriptItem};
pub use transpile::Lang;
