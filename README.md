# xRename

一个 Windows 桌面端的批量重命名工具，用 Rust + [gpui-kit](https://crates.io/crates/gpui-kit) 写成。

重命名逻辑不再靠预设的几种「模式」，而是**写一段代码**或者**填一个模板**：脚本在运行时被转译成 Rust、编译成动态库再调用，所以速度接近原生，同时保住了任意复杂的命名逻辑。

## 功能

**输入**
- 拖入文件、文件夹，或用按钮选择。
- 拖入文件夹时询问：重命名文件夹本身 / 重命名其中的文件 / 跳过。选「其中的文件」时可以勾选**是否递归遍历子文件夹**（默认只处理当前这一层）。

**三栏布局**

| 左：命名方法 | 中：脚本编辑 | 右：条目表格 |
|---|---|---|
| 自定义脚本 / 自定义规则 两种方式 | 只写函数体，返回新文件名 | 原文件名、新文件名、扩展名、index、大小、路径、错误 |

**脚本**
- 支持 JavaScript、C++、Python、Rust、Java 五种语言的写法（仅函数体，`item`、`index` 由运行时注入，`return` 一个字符串即可）。
- 编辑区有语法高亮、`item.` 后的字段自动补全（↑↓ 选择、Tab/Enter 确认、Esc 关闭）。
- 切换语言时自动带出该语言的示例函数体。

**规则模板**
- 形如 `01.{name}.{ext}`，支持 `{name}` `{ext}` `{path}` `{index}` `{size}` `{createTime}` `{mediaDate}` `{mediaType}` 等变量，面板里有变量说明弹窗。

**预览与执行**
- 「应用」只做预览，不动磁盘；「开始处理」才真正改名。
- 两阶段重命名 + 冲突自愈，`A → B` 和 `B → A` 这类互换不会互相覆盖。
- 支持「撤销上次处理」，把上一批改过的名字原样改回去。

**表格**
- 列可显隐（列设置），文件名/新文件名/路径/错误四列固定不可隐藏。
- 列宽可拖动，双击列边界按内容自适应，一键恢复默认列宽。

## 构建

需要 Rust（MSVC 工具链）与 Windows SDK 的 `rc.exe`（用于把图标和版本信息写进 exe，查找逻辑在 `build.rs` 里，也可以用 `RC_PATH` 环境变量直接指定）。

```bash
cargo build            # debug
cargo build --release  # release
cargo test
```

运行：

```bash
cargo run -- "C:\path\to\a.txt" "C:\path\to\b.txt"
```

## 打包

```powershell
.\pack.ps1 0.2.1              # 或 pack.cmd 0.2.1
.\pack.ps1 0.2.1 -SkipTests   # 跳过测试
.\pack.ps1 0.2.1 -NoZip       # 只出 exe，不压缩
```

脚本会改写 `Cargo.toml` 的 `version`（版本号的唯一真源，`build.rs` 与界面都读它），跑测试、`cargo build --release`，把 exe 收进 `dist/xRename-v<版本>-win-x64/`，再打成 zip 并打印大小与 SHA256。

## 目录

```
src/
  main.rs        入口、窗口与 logo
  app.rs         主界面：三栏布局、状态、动作
  model.rs       条目模型（扩展名分类、时间、规则模板展开）
  table.rs       DataTable 的 delegate（列定义、单元格、双击自适应）
  columns.rs     列的显隐与宽度
  complete.rs    自动补全的候选列表
  highlight.rs   语法高亮
  lang_select.rs 语言下拉框
  script/
    mod.rs       脚本缓存与调用入口
    transpile.rs 五种语言 → Rust 的转译
    runtime.rs   转译结果编译成 cdylib 并按内容哈希缓存
assets/          图标（由根目录 logo.png 生成）
build.rs         写入 exe 图标与版本信息
```

## 说明

- 脚本在独立的动态库里执行，调用侧用 `catch_unwind` 兜住 panic，所以 release profile 特意没有开 `panic = "abort"`。
- 目前只在 Windows 上验证过；gpui 本身支持 macOS / Linux，但图标、打包脚本是 Windows 专用的。
