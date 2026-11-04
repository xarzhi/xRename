//! 主界面：顶部工具栏 + 三栏（脚本 / 表格 / 日志）。

use std::path::PathBuf;

use gpui_kit::base::input::InputEvent;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Editor, EditorState, Input, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::table::{DataTable, TableEvent, TableState};
use gpui_kit::component::{ActiveTheme, IndexPath, Sizable, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::columns::{Col, Columns, Widths};
use crate::complete;
use crate::lang_select::{self, LangItem};
use crate::model::{self, Entry, Origin};
use crate::script::{Lang, ScriptCache};
use crate::table::EntryTable;

const ROW_H: f32 = 34.0;
const SCRIPT_PANEL_W: f32 = 400.0;
const SIDE_PANEL_W: f32 = 290.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Ok,
    Error,
}

/// 改名方式。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    /// 自定义脚本：写函数体，翻译成 Rust 执行
    Script,
    /// 自定义规则：`{seq:3}_{stem}.{ext}` 这样的模板
    Rule,
}

impl Method {
    fn label(self) -> &'static str {
        match self {
            Method::Script => "自定义脚本",
            Method::Rule => "自定义规则",
        }
    }
}

pub struct RenameApp {
    entries: Vec<Entry>,
    method: Method,
    lang: Lang,
    /// 语言下拉框
    lang_select: Entity<SelectState<Vec<LangItem>>>,
    /// 下拉框选项变化的订阅，丢掉就失效
    lang_sub: Option<Subscription>,
    editor: Entity<EditorState>,
    rule: Entity<InputState>,
    cache: ScriptCache,
    keep_ext: bool,
    /// 上次「应用」时的输入指纹，用来判断预览是否过期
    applied_sig: Option<String>,
    rust_preview: String,
    show_rust: bool,
    show_vars: bool,
    /// 表格列设置弹窗
    show_columns: bool,
    columns: Columns,
    /// 各列当前宽度（拖动表头时由表格写回来）
    widths: Widths,
    /// 文件表格。表格要拿着主界面的弱引用，只能等主界面构造完再建，所以是 Option。
    table: Option<Entity<TableState<EntryTable>>>,
    /// 表格事件的订阅，丢掉就失效
    table_sub: Option<Subscription>,
    /// 上次真正改名留下的 `(改名前, 改名后)`，用于撤销
    last_commit: Vec<(PathBuf, PathBuf)>,
    /// `item.` 补全浮窗：是否展开
    completion_open: bool,
    /// 待替换片段的起始字节位置
    completion_start: usize,
    /// 已经输入的过滤前缀
    completion_query: String,
    /// 当前高亮的候选项下标
    completion_sel: usize,
    /// 监听编辑器内容变化，内容一变就重算补全
    editor_sub: Option<Subscription>,
    /// 按键拦截器的订阅，丢掉就会被取消
    key_sub: Option<Subscription>,
    status: String,
    level: Level,
    log: Vec<(Level, String)>,
    pending: Vec<PathBuf>,
    /// 添加文件夹时是否连子文件夹里的文件一起收（弹窗里勾选，默认不勾）
    folder_recursive: bool,
}

impl RenameApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let lang = Lang::Js;
        let lang_select = cx.new(|cx| {
            SelectState::new(
                lang_select::items(),
                Some(IndexPath::new(lang_select::index_of(lang))),
                window,
                cx,
            )
        });
        let editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .line_number(false)
                .folding(false)
                .language(lang.id())
                .placeholder("在这里写函数体……")
                .default_value(lang.template());
            // 语法高亮：语言名由 `set_highlighter` 传进工厂
            state.set_highlighter_factory(crate::highlight::factory(), cx);
            state
        });
        let rule = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("{seq:3}_{stem}.{ext}")
                .default_value("{seq:3}_{stem}.{ext}")
        });
        let mut this = Self {
            entries: Vec::new(),
            method: Method::Script,
            lang,
            lang_select,
            lang_sub: None,
            editor,
            rule,
            cache: ScriptCache::new(),
            keep_ext: true,
            applied_sig: None,
            rust_preview: String::new(),
            show_rust: false,
            show_vars: false,
            show_columns: false,
            columns: Columns::default(),
            widths: Widths::default(),
            table: None,
            table_sub: None,
            last_commit: Vec::new(),
            completion_open: false,
            completion_start: 0,
            completion_query: String::new(),
            completion_sel: 0,
            editor_sub: None,
            key_sub: None,
            status: "把文件或文件夹拖进窗口，或者用上方的按钮添加".to_string(),
            level: Level::Info,
            log: Vec::new(),
            pending: Vec::new(),
            folder_recursive: false,
        };
        this.push_log(Level::Info, "准备就绪，默认用 JavaScript 脚本示例");

        // 下拉框选中某一项：切语言（示例代码、高亮规则一起换）
        let lang_select = this.lang_select.clone();
        this.lang_sub = Some(cx.subscribe_in(
            &lang_select,
            window,
            |this, _, event: &SelectEvent<Vec<LangItem>>, window, cx| {
                if let SelectEvent::Confirm(Some(lang)) = event {
                    this.switch_lang(*lang, window, cx);
                }
            },
        ));

        // 编辑器内容一变（打字、删除、粘贴）就重算补全上下文
        let editor = this.editor.clone();
        this.editor_sub = Some(cx.subscribe_in(
            &editor,
            window,
            |this, _, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.refresh_completion(cx);
                }
            },
        ));

        // 浮窗打开时，上下键 / Tab / 回车要优先给补全用，
        // 必须在按键解析成动作之前拦下来，所以走 keystroke 拦截器。
        let weak = cx.weak_entity();
        this.key_sub = Some(cx.intercept_keystrokes(move |event, window, cx| {
            if let Some(view) = weak.upgrade() {
                view.update(cx, |this, cx| this.on_completion_key(event, window, cx));
            }
        }));

        // 命令行传入的路径直接进列表，并按当前脚本算一次预览
        let args: Vec<PathBuf> = std::env::args_os()
            .skip(1)
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect();
        if !args.is_empty() {
            this.add_paths(args);
            if !this.entries.is_empty() {
                this.apply_now(cx);
            }
        }
        this
    }

    // ------------------------------------------------------------------
    // 状态与操作
    // ------------------------------------------------------------------

    fn push_log(&mut self, level: Level, msg: impl Into<String>) {
        self.log.insert(0, (level, msg.into()));
        if self.log.len() > 200 {
            self.log.truncate(200);
        }
    }

    fn set_status(&mut self, level: Level, msg: impl Into<String>) {
        self.status = msg.into();
        self.level = level;
    }

    // ------------------------------------------------------------------
    // 表格
    // ------------------------------------------------------------------

    /// 拿到表格实体，第一次用到时才建。
    ///
    /// 表格的 delegate 要拿主界面的弱引用，而主界面在自己的构造 / 渲染里
    /// 是读不到自己的，所以推迟到第一次渲染再建。
    fn table_entity(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TableState<EntryTable>> {
        if let Some(table) = &self.table {
            return table.clone();
        }
        let app = cx.entity().downgrade();
        let delegate = EntryTable::new(
            app,
            crate::table::rows_of(&self.entries),
            crate::columns::visible(&self.columns),
            self.widths,
        );
        let table = cx.new(|cx| {
            // 行选中、整列选中、拖列换位、点表头排序都不需要：
            // 选中由 entry.selected 自己管，点行就是勾选
            TableState::new(delegate, window, cx)
                .row_selectable(false)
                .col_selectable(false)
                .col_movable(false)
                .sortable(false)
                .loop_selection(false)
        });
        // 拖完列宽 / 双击自适应后，表格把新宽度全量发回来，存下来供重建时用
        self.table_sub = Some(cx.subscribe_in(
            &table,
            window,
            |this, _, event: &TableEvent, _window, cx| {
                if let TableEvent::ColumnWidthsChanged(widths) = event {
                    this.store_widths(widths);
                    cx.notify();
                }
            },
        ));
        self.table = Some(table.clone());
        table
    }

    /// 把表格报上来的宽度按当前列顺序存进 `widths`。
    fn store_widths(&mut self, widths: &[Pixels]) {
        let cols = crate::columns::visible(&self.columns);
        for (col, width) in cols.iter().zip(widths.iter()) {
            self.widths.set(*col, width.as_f32());
        }
    }

    /// 表格内容变了：重拍一份快照交给表格，再让它重画。
    fn table_changed(&mut self, cx: &mut Context<Self>) {
        let rows = crate::table::rows_of(&self.entries);
        if let Some(table) = &self.table {
            table.update(cx, |state, cx| {
                state.delegate_mut().set_rows(rows);
                cx.notify();
            });
        }
        cx.notify();
    }

    /// 列定义（显隐 / 宽度）变了：把列和宽度交给表格，让它重建列。
    fn refresh_table(&mut self, cx: &mut Context<Self>) {
        let cols = crate::columns::visible(&self.columns);
        let widths = self.widths;
        if let Some(table) = &self.table {
            table.update(cx, |state, cx| {
                state.delegate_mut().set_columns(cols, widths);
                state.refresh(cx);
            });
        }
        cx.notify();
    }

    /// 点行：切换这一行的选中状态。
    pub(crate) fn toggle_row(&mut self, index: usize, cx: &mut Context<Self>) {
        self.toggle_select(index);
        self.table_changed(cx);
    }

    fn reset_all_widths(&mut self, cx: &mut Context<Self>) {
        self.widths.reset_all();
        self.set_status(Level::Info, "所有列宽已恢复默认");
        self.refresh_table(cx);
    }

    fn switch_lang(&mut self, lang: Lang, window: &mut Window, cx: &mut Context<Self>) {
        if self.lang == lang {
            return;
        }
        let current = self.editor.read(cx).value().to_string();
        let custom = !current.trim().is_empty()
            && !Lang::all()
                .iter()
                .any(|l| l.template().trim() == current.trim());
        self.lang = lang;

        // 切换语言时，函数体跟着换成这门语言的示例，高亮规则也一起换
        let template = lang.template();
        self.editor.update(cx, |state, cx| {
            state.set_value(template, window, cx);
            state.set_highlighter(lang.id(), cx);
        });
        let handle = Focusable::focus_handle(&*self.editor.read(cx), cx).clone();
        handle.focus(window, cx);
        self.completion_open = false;
        self.completion_query.clear();
        self.applied_sig = None;

        if custom {
            let msg = format!(
                "脚本语言切换到 {}，函数体已换成该语言示例（原来的自定义代码被替换）",
                lang.label()
            );
            self.set_status(Level::Info, format!("脚本语言切换到 {}", lang.label()));
            self.push_log(Level::Info, msg);
        } else {
            let msg = format!("脚本语言切换到 {}，示例已同步", lang.label());
            self.set_status(Level::Info, msg.clone());
            self.push_log(Level::Info, msg);
        }
        cx.notify();
    }

    fn switch_method(&mut self, method: Method, cx: &mut Context<Self>) {
        if self.method == method {
            return;
        }
        self.method = method;
        self.completion_open = false;
        self.completion_query.clear();
        self.applied_sig = None;
        self.set_status(Level::Info, format!("改名方式：{}", method.label()));
        cx.notify();
    }

    // ------------------------------------------------------------------
    // item. 自动补全
    // ------------------------------------------------------------------

    /// 文本或光标变了：重新判断要不要弹浮窗、过滤词是什么。
    fn refresh_completion(&mut self, cx: &mut Context<Self>) {
        if self.method != Method::Script {
            if self.completion_open {
                self.completion_open = false;
                cx.notify();
            }
            return;
        }
        let (text, cursor) = {
            let state = self.editor.read(cx);
            (state.value().to_string(), state.cursor())
        };
        match complete::context_at(&text, cursor) {
            Some(ctx) => {
                let reopened = !self.completion_open || ctx.query != self.completion_query;
                if reopened {
                    self.completion_sel = 0;
                }
                self.completion_open = true;
                self.completion_start = ctx.start;
                self.completion_query = ctx.query;
                if reopened {
                    cx.notify();
                }
                // 光标坐标来自上一帧的布局，等这一帧布局落定后再补一次重绘，
                // 浮窗才会正好贴在新光标下面。
                self.schedule_completion_repaint(cx);
            }
            None => {
                if self.completion_open {
                    self.completion_open = false;
                    self.completion_query.clear();
                    cx.notify();
                }
            }
        }
    }

    /// 布局稳定后补一帧，让浮窗对准最新的光标位置。
    fn schedule_completion_repaint(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(24))
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// 按键拦截：浮窗打开时，上下键换候选项，Tab / 回车确认，Esc 关闭。
    fn on_completion_key(
        &mut self,
        event: &KeystrokeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.completion_open {
            return;
        }
        let total = complete::filtered(&self.completion_query).len();
        if total == 0 {
            return;
        }
        match event.keystroke.key.as_str() {
            "down" => {
                self.completion_sel = (self.completion_sel + 1) % total;
                cx.stop_propagation();
                cx.notify();
            }
            "up" => {
                self.completion_sel = (self.completion_sel + total - 1) % total;
                cx.stop_propagation();
                cx.notify();
            }
            "tab" | "enter" => {
                cx.stop_propagation();
                self.accept_completion(window, cx);
            }
            "escape" => {
                self.completion_open = false;
                self.completion_query.clear();
                cx.stop_propagation();
                cx.notify();
            }
            _ => {}
        }
    }

    /// 把当前高亮的字段写进编辑器，替换掉 `item.` 后面已经输入的前缀。
    fn accept_completion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fields = complete::filtered(&self.completion_query);
        let Some(field) = fields
            .get(self.completion_sel.min(fields.len().saturating_sub(1)))
            .copied()
        else {
            self.completion_open = false;
            cx.notify();
            return;
        };

        let (text, cursor) = {
            let state = self.editor.read(cx);
            (state.value().to_string(), state.cursor())
        };
        let start = self.completion_start.min(text.len());
        let cursor = cursor.min(text.len());
        if !text.is_char_boundary(start) || cursor < start {
            self.completion_open = false;
            cx.notify();
            return;
        }

        let mut next = String::with_capacity(text.len() + field.name.len());
        next.push_str(&text[..start]);
        next.push_str(field.name);
        next.push_str(&text[cursor..]);
        let caret = start + field.name.len();

        self.completion_open = false;
        self.completion_query.clear();
        self.applied_sig = None;
        self.editor.update(cx, |state, cx| {
            state.set_value(next, window, cx);
            state.set_selected_range(caret..caret, cx);
        });
        let handle = Focusable::focus_handle(&*self.editor.read(cx), cx).clone();
        handle.focus(window, cx);
        cx.notify();
    }

    /// 补全浮窗，锚在光标下方。
    fn completion_popup(&self, window: &Window, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if !self.completion_open || self.method != Method::Script {
            return None;
        }
        let fields = complete::filtered(&self.completion_query);
        if fields.is_empty() {
            return None;
        }
        if !self.editor.read(cx).has_selection_focus(window, cx) {
            return None;
        }

        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let bg = cx.theme().background;
        let accent = cx.theme().accent;
        let mono = cx.theme().mono_font_family.clone();

        let selected = self.completion_sel.min(fields.len() - 1);
        let mut list = v_flex().w_full().gap_1();
        for (i, field) in fields.iter().enumerate() {
            let mut row = h_flex()
                .id(SharedString::from(format!("completion-{i}")))
                .w_full()
                .px_2()
                .py(px(6.0))
                .gap_2()
                .items_center()
                .rounded_md();
            if i == selected {
                row = row.bg(accent);
            }
            list = list.child(
                row.child(
                    div()
                        .text_size(px(12.0))
                        .font_family(mono.clone())
                        .text_color(fg)
                        .child(field.name),
                )
                .child(div().text_size(px(9.0)).text_color(muted).child(field.ty))
                .child(div().flex_1())
                .child(div().text_size(px(9.0)).text_color(muted).child(field.desc))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.completion_sel = i;
                    this.accept_completion(window, cx);
                })),
            );
        }

        let popup = v_flex()
            .w(px(320.0))
            .p_1()
            .gap_1()
            .rounded_md()
            .border_1()
            .border_color(border)
            .bg(bg)
            .child(list)
            .child(
                div()
                    .w_full()
                    .px_2()
                    .pt(px(4.0))
                    .border_t_1()
                    .border_color(border)
                    .text_size(px(9.0))
                    .text_color(muted)
                    .child("↑↓ 选择 · Tab 确认 · Esc 关闭"),
            );

        let (caret, line_h) = self.editor.read(cx).cursor_layout()?;
        Some(
            anchored()
                .position(point(caret.origin.x, caret.origin.y + line_h))
                .anchor(Anchor::TopLeft)
                .snap_to_window()
                .child(popup),
        )
    }

    /// 当前输入（方式 + 内容）的指纹，变了就说明预览过期了。
    fn current_sig(&self, cx: &Context<Self>) -> String {
        match self.method {
            Method::Script => format!("script:{}:{}", self.lang.short(), self.editor.read(cx).value()),
            Method::Rule => format!("rule:{}", self.rule.read(cx).value()),
        }
    }

    fn add_paths(&mut self, paths: Vec<PathBuf>) {
        let mut added = 0usize;
        let mut folders = 0usize;
        for path in paths {
            let Ok(meta) = std::fs::metadata(&path) else {
                self.push_log(Level::Error, format!("无法读取: {}", path.display()));
                continue;
            };
            if meta.is_dir() {
                if !self.pending.iter().any(|p| p.as_path() == path.as_path()) {
                    self.pending.push(path);
                    folders += 1;
                }
            } else if self.push_entry(&path, Origin::File) {
                added += 1;
            }
        }
        if added > 0 {
            self.push_log(Level::Ok, format!("已加入 {added} 个文件"));
        }
        if folders > 0 {
            self.push_log(
                Level::Info,
                format!("{folders} 个文件夹待确认：重命名里面的内容，还是文件夹本身？"),
            );
        }
        if added + folders > 0 {
            self.applied_sig = None;
            self.set_status(Level::Info, format!("当前共 {} 个条目", self.entries.len()));
        }
    }

    fn push_entry(&mut self, path: &std::path::Path, origin: Origin) -> bool {
        if self.entries.iter().any(|e| e.path.as_path() == path) {
            return false;
        }
        match Entry::from_path(path, origin) {
            Some(entry) => {
                self.entries.push(entry);
                true
            }
            None => {
                self.push_log(Level::Error, format!("无法读取元信息: {}", path.display()));
                false
            }
        }
    }

    fn skip_pending(&mut self, cx: &mut Context<Self>) {
        if self.pending.is_empty() {
            return;
        }
        let folder = self.pending.remove(0);
        self.push_log(Level::Info, format!("跳过: {}", folder.display()));
        cx.notify();
    }

    fn resolve_pending(&mut self, as_children: bool, cx: &mut Context<Self>) {
        if self.pending.is_empty() {
            return;
        }
        let folder = self.pending.remove(0);
        if as_children {
            let recursive = self.folder_recursive;
            let children = model::collect_children(&folder, recursive);
            let count = children.len();
            let existing: Vec<PathBuf> = self.entries.iter().map(|e| e.path.clone()).collect();
            let mut added = 0usize;
            for entry in children {
                if !existing.contains(&entry.path) {
                    self.entries.push(entry);
                    added += 1;
                }
            }
            let scope = if recursive { "（含子文件夹）" } else { "" };
            self.push_log(
                Level::Ok,
                format!("{} 内加入 {added} 个文件{scope}", folder.display()),
            );
            if count == 0 {
                self.push_log(
                    Level::Info,
                    format!("{} 里没有可处理的文件", folder.display()),
                );
            } else if added == 0 {
                self.push_log(Level::Info, "这些文件已经在列表里了");
            }
        } else if self.push_entry(&folder, Origin::FolderSelf) {
            self.push_log(Level::Ok, format!("加入文件夹本身: {}", folder.display()));
        }
        self.applied_sig = None;
        self.set_status(Level::Info, format!("当前共 {} 个条目", self.entries.len()));
        self.table_changed(cx);
    }

    fn clear_all(&mut self, cx: &mut Context<Self>) {
        self.entries.clear();
        self.applied_sig = None;
        self.rust_preview.clear();
        self.set_status(Level::Info, "已清空列表");
        self.push_log(Level::Info, "清空列表");
        self.table_changed(cx);
    }

    fn remove_selected(&mut self, cx: &mut Context<Self>) {
        let before = self.entries.len();
        self.entries.retain(|e| !e.selected);
        let removed = before - self.entries.len();
        if removed > 0 {
            self.applied_sig = None;
            let msg = format!("移除 {removed} 个条目");
            self.set_status(Level::Info, msg.clone());
            self.push_log(Level::Info, msg);
        }
        self.table_changed(cx);
    }

    fn toggle_select(&mut self, index: usize) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.selected = !entry.selected;
        }
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        let all_selected = self.entries.iter().all(|e| e.selected);
        for entry in self.entries.iter_mut() {
            entry.selected = !all_selected;
        }
        self.table_changed(cx);
    }

    /// 应用：只计算新名字（预览），不碰磁盘。
    fn apply_now(&mut self, cx: &mut Context<Self>) {
        if self.entries.is_empty() {
            self.set_status(Level::Error, "列表为空，先添加文件");
            cx.notify();
            return;
        }
        let sig = self.current_sig(cx);
        match self.method {
            Method::Script => {
                let body = self.editor.read(cx).value().to_string();
                if body.trim().is_empty() {
                    self.set_status(Level::Error, "脚本函数体为空");
                    cx.notify();
                    return;
                }
                match self.cache.build(self.lang, &body) {
                    Ok(script) => {
                        self.rust_preview = script.rust_code.clone();
                        model::apply_script(&mut self.entries, &script, self.keep_ext);
                        self.applied_sig = Some(sig);
                        self.report_apply("脚本");
                    }
                    Err(err) => {
                        self.applied_sig = None;
                        self.set_status(Level::Error, "脚本编译失败，错误详情在右侧日志");
                        self.push_log(Level::Error, format!("脚本编译失败：\n{err}"));
                    }
                }
            }
            Method::Rule => {
                let pattern = self.rule.read(cx).value().to_string();
                match model::apply_rule(&mut self.entries, &pattern) {
                    Ok(()) => {
                        self.applied_sig = Some(sig);
                        self.report_apply("规则");
                    }
                    Err(err) => {
                        self.applied_sig = None;
                        let msg = format!("规则有问题：{err}");
                        self.set_status(Level::Error, msg.clone());
                        self.push_log(Level::Error, msg);
                    }
                }
            }
        }
        self.table_changed(cx);
    }

    fn report_apply(&mut self, what: &str) {
        let errors = self.entries.iter().filter(|e| e.error.is_some()).count();
        let changed = self
            .entries
            .iter()
            .filter(|e| e.error.is_none() && e.has_change())
            .count();
        if errors > 0 {
            let msg = format!("{what}已应用：{changed} 项待改名，{errors} 项有问题");
            self.set_status(Level::Error, msg.clone());
            self.push_log(Level::Error, msg);
        } else {
            let msg = format!("{what}已应用：{changed} 项待改名");
            self.set_status(Level::Ok, msg.clone());
            self.push_log(Level::Ok, msg);
        }
    }

    /// 开始处理：真正改磁盘上的名字。
    fn run_rename(&mut self, cx: &mut Context<Self>) {
        if self.entries.is_empty() {
            self.set_status(Level::Error, "列表为空，先添加文件");
            return;
        }
        let sig = self.current_sig(cx);
        if self.applied_sig.as_deref() != Some(sig.as_str()) {
            self.apply_now(cx);
        }
        if self.entries.iter().all(|e| !e.has_change()) {
            self.set_status(Level::Info, "没有需要修改的文件名（没有产生变化）");
            cx.notify();
            return;
        }

        let before: Vec<PathBuf> = self.entries.iter().map(|e| e.path.clone()).collect();
        let report = model::commit(&mut self.entries);

        // 记下这次真正变了的部分，供「撤销上次处理」用
        let mut records = Vec::new();
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.path != before[i] {
                records.push((before[i].clone(), entry.path.clone()));
            }
        }
        if !records.is_empty() {
            self.last_commit = records;
        }

        let msg = format!(
            "已重命名 {} 个，失败 {} 个，跳过 {} 个",
            report.renamed, report.failed, report.skipped
        );
        let level = if report.failed > 0 { Level::Error } else { Level::Ok };
        self.push_log(level, msg.clone());
        self.set_status(level, msg);
        self.table_changed(cx);
    }

    /// 撤销上次处理：把上次改过的名字原样改回去。
    fn undo_last(&mut self, cx: &mut Context<Self>) {
        if self.last_commit.is_empty() {
            self.set_status(Level::Info, "没有可撤销的处理");
            self.push_log(Level::Info, "没有可撤销的处理（只有点过「开始处理」才记录）");
            cx.notify();
            return;
        }

        let records = std::mem::take(&mut self.last_commit);
        let report = model::undo(&records);

        let mut keep: Vec<(PathBuf, PathBuf)> = Vec::new();
        for (i, (old, new)) in records.iter().enumerate() {
            if report.undone.contains(&i) {
                if let Some(entry) = self.entries.iter_mut().find(|e| &e.path == new) {
                    entry.path = old.clone();
                    entry.refresh();
                    entry.renamed = false;
                    entry.new_name.clear();
                    entry.error = None;
                }
            } else {
                keep.push((old.clone(), new.clone()));
            }
        }
        self.last_commit = keep;
        self.applied_sig = None;

        let msg = if report.failed > 0 {
            format!(
                "已撤销 {} 个，{} 个没撤回来（原名字可能被占用）",
                report.undone.len(),
                report.failed
            )
        } else {
            format!("已撤销上次的 {} 个改名", report.undone.len())
        };
        let level = if report.failed > 0 { Level::Error } else { Level::Ok };
        self.set_status(level, msg.clone());
        self.push_log(level, msg);
        self.table_changed(cx);
    }

    fn pick_files(&mut self, cx: &mut Context<Self>) {
        if let Some(files) = rfd::FileDialog::new()
            .set_title("选择要重命名的文件")
            .pick_files()
        {
            self.add_paths(files);
        }
        self.table_changed(cx);
    }

    fn pick_folder(&mut self, cx: &mut Context<Self>) {
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("选择文件夹")
            .pick_folder()
        {
            self.add_paths(vec![folder]);
        }
        self.table_changed(cx);
    }

    // ------------------------------------------------------------------
    // 视图
    // ------------------------------------------------------------------

    /// 工具条：只放操作按钮。标题和 logo 交给系统标题栏，别重复画一遍。
    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;
        let border = cx.theme().border;

        h_flex()
            .w_full()
            .h(px(52.0))
            .px_4()
            .gap_2()
            .items_center()
            .bg(bg)
            .border_b_1()
            .border_color(border)
            .child(
                Button::new("add-files")
                    .outline()
                    .label("添加文件")
                    .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
            )
            .child(
                Button::new("add-folder")
                    .outline()
                    .label("添加文件夹")
                    .on_click(cx.listener(|this, _, _, cx| this.pick_folder(cx))),
            )
            .child(
                Button::new("remove-selected")
                    .ghost()
                    .label("移除选中")
                    .on_click(cx.listener(|this, _, _, cx| this.remove_selected(cx))),
            )
            .child(
                Button::new("clear")
                    .ghost()
                    .label("清空")
                    .on_click(cx.listener(|this, _, _, cx| this.clear_all(cx))),
            )
            .child(div().w(px(1.0)).h(px(22.0)).bg(border).mx_2())
            .child(
                Button::new("undo")
                    .ghost()
                    .label("撤销上次处理")
                    .on_click(cx.listener(|this, _, _, cx| this.undo_last(cx))),
            )
            .child(
                Button::new("start")
                    .primary()
                    .label("开始处理")
                    .on_click(cx.listener(|this, _, _, cx| this.run_rename(cx))),
            )
    }

    fn method_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let fg = cx.theme().foreground;
        let mono = cx.theme().mono_font_family.clone();
        let muted_bg = cx.theme().muted;

        // 改名方式：两个复选框，二选一
        let method_row = h_flex()
            .gap_4()
            .flex_wrap()
            .child(
                Checkbox::new("method-script")
                    .label(Method::Script.label())
                    .checked(self.method == Method::Script)
                    .on_click(cx.listener(|this, _, _, cx| this.switch_method(Method::Script, cx))),
            )
            .child(
                Checkbox::new("method-rule")
                    .label(Method::Rule.label())
                    .checked(self.method == Method::Rule)
                    .on_click(cx.listener(|this, _, _, cx| this.switch_method(Method::Rule, cx))),
            );

        // 脚本语言：下拉框选择
        let lang_row = h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.0))
                    .text_color(muted)
                    .child("脚本语言"),
            )
            .child(
                div().flex_1().min_w(px(0.0)).child(
                    Select::new(&self.lang_select)
                        .small()
                        .menu_width(px(180.0))
                        .w_full(),
                ),
            );

        let mut panel = v_flex()
            .w(px(SCRIPT_PANEL_W))
            .h_full()
            .min_h(px(0.0))
            .overflow_hidden()
            .bg(bg)
            .gap_2()
            .p_3()
            .border_r_1()
            .border_color(border)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child("重命名方法"),
                    )
                    .child(
                        Button::new("show-vars")
                            .ghost()
                            .label("变量说明")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_vars = true;
                                cx.notify();
                            })),
                    ),
            )
            .child(method_row)
            .child(
                div()
                    .text_size(px(10.0))
                    .line_height(px(15.0))
                    .text_color(muted)
                    .child(match self.method {
                        Method::Script => {
                            "只写函数体，必须返回字符串。\n输入 item. 弹出字段补全（↑↓ 选，Tab 确认）"
                        }
                        Method::Rule => {
                            "花括号里是变量，花括号外原样保留。\n例：{seq:3}_{stem}.{ext}"
                        }
                    }),
            );

        match self.method {
            Method::Script => {
                let code_block = v_flex()
                    .flex_1()
                    .min_h(px(130.0))
                    .rounded_md()
                    .overflow_hidden()
                    .border_1()
                    .border_color(border)
                    .child(
                        div()
                            .w_full()
                            .h(px(24.0))
                            .px_2()
                            .flex()
                            .items_center()
                            .bg(muted_bg)
                            .text_size(px(11.0))
                            .font_family(mono.clone())
                            .text_color(muted)
                            .child(self.lang.signature()),
                    )
                    .child(
                        div().flex_1().min_h(px(80.0)).child(
                            Editor::new(&self.editor)
                                .bordered(false)
                                .h_full()
                                .w_full()
                                .text_size(px(12.0))
                                .font_family(mono.clone()),
                        ),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(24.0))
                            .px_2()
                            .flex()
                            .items_center()
                            .bg(muted_bg)
                            .text_size(px(11.0))
                            .font_family(mono.clone())
                            .text_color(muted)
                            .child(self.lang.footer()),
                    );

                let keep_ext_btn = Button::new("keep-ext")
                    .label(if self.keep_ext {
                        "自动补全扩展名 · 开"
                    } else {
                        "自动补全扩展名 · 关"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.keep_ext = !this.keep_ext;
                        this.applied_sig = None;
                        cx.notify();
                    }));
                let keep_ext_btn = if self.keep_ext {
                    keep_ext_btn.primary()
                } else {
                    keep_ext_btn.ghost()
                };

                let rust_toggle = Button::new("toggle-rust")
                    .ghost()
                    .label(if self.show_rust {
                        "隐藏转换结果"
                    } else {
                        "查看转换结果"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_rust = !this.show_rust;
                        cx.notify();
                    }));

                panel = panel
                    .child(lang_row)
                    .child(code_block)
                    .child(h_flex().gap_1().child(keep_ext_btn).child(rust_toggle));

                if self.show_rust {
                    panel = panel.child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(muted)
                                    .child("转换后的 Rust（函数体）"),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .max_h(px(120.0))
                                    .overflow_hidden()
                                    .rounded_md()
                                    .bg(muted_bg)
                                    .p_2()
                                    .text_size(px(10.0))
                                    .font_family(mono.clone())
                                    .child(if self.rust_preview.is_empty() {
                                        "点击「应用」后显示".to_string()
                                    } else {
                                        self.rust_preview.clone()
                                    }),
                            ),
                    );
                }
            }
            Method::Rule => {
                let mut var_list = v_flex().gap_1();
                for (var, desc, sample) in model::RULE_VARS.iter() {
                    var_list =
                        var_list.child(rule_var_row(var, desc, sample, fg, muted, mono.clone()));
                }

                panel = panel
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .w_full()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(border)
                                    .px_2()
                                    .child(
                                        Input::new(&self.rule)
                                            .w_full()
                                            .text_size(px(12.0))
                                            .font_family(mono.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .line_height(px(15.0))
                                    .text_color(muted)
                                    .child("试一个：{seq:2}_{name} · {stem}_v2.{ext}"),
                            ),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(fg)
                                    .child("可用变量"),
                            )
                            .child(var_list),
                    )
                    .child(div().flex_1());
            }
        }

        panel.child(
            Button::new("apply")
                .primary()
                .w_full()
                .label("应用（仅预览，不改动文件）")
                .on_click(cx.listener(|this, _, _, cx| this.apply_now(cx))),
        )
    }

    fn table_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let fg = cx.theme().foreground;
        let count = self.entries.len();
        let hidden = self.columns.hidden();

        let body: AnyElement = if count == 0 {
            self.empty_state(cx).into_any_element()
        } else {
            // 列宽拖拽、双击自适应、横向滚动全交给 DataTable
            let table = self.table_entity(window, cx);
            DataTable::new(&table)
                .stripe(false)
                .bordered(false)
                .scrollbar_visible(true, true)
                .header_row_height(px(30.0))
                .with_size(px(ROW_H))
                .into_any_element()
        };

        v_flex()
            .id("table-panel")
            .flex_1()
            .h_full()
            .min_h(px(0.0))
            .min_w(px(320.0))
            .border_r_1()
            .border_color(border)
            .child(
                h_flex()
                    .w_full()
                    .h(px(38.0))
                    .px_3()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child("待处理列表"),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new("columns")
                                    .ghost()
                                    .label(if hidden > 0 {
                                        format!("列设置 · 隐藏 {hidden}")
                                    } else {
                                        "列设置".to_string()
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_columns = true;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("select-all")
                                    .ghost()
                                    .label("全选/取消")
                                    .on_click(cx.listener(|this, _, _, cx| this.select_all(cx))),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(muted)
                                    .child(format!("{count} 个条目")),
                            ),
                    ),
            )
            // 表格（或空状态）撑满剩下的高度
            .child(div().flex_1().min_h(px(0.0)).w_full().child(body))
    }

    fn empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let primary = cx.theme().primary;
        let border = cx.theme().border;
        v_flex()
            .flex_1()
            .h_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(
                div()
                    .text_size(px(15.0))
                    .text_color(primary)
                    .child("把文件 / 文件夹拖到这里"),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(muted)
                    .child("拖入文件夹时会询问：重命名里面的条目，还是重命名文件夹本身"),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .rounded_full()
                    .border_1()
                    .border_color(border)
                    .text_size(px(10.0))
                    .text_color(muted)
                    .child("可以一次拖入多个文件与文件夹"),
            )
    }

    fn side_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let fg = cx.theme().foreground;
        let primary = cx.theme().primary;
        let danger = cx.theme().danger;

        let total = self.entries.len();
        let issues = self.entries.iter().filter(|e| e.error.is_some()).count();
        let changed = self
            .entries
            .iter()
            .filter(|e| e.error.is_none() && e.has_change())
            .count();
        let done = self.entries.iter().filter(|e| e.renamed).count();

        let mut logs = v_flex().gap_1();
        for (level, msg) in self.log.iter().take(40) {
            let color = match level {
                Level::Ok => primary,
                Level::Error => danger,
                Level::Info => muted,
            };
            logs = logs.child(
                div()
                    .text_size(px(10.0))
                    .line_height(px(15.0))
                    .text_color(color)
                    .child(msg.clone()),
            );
        }

        v_flex()
            .w(px(SIDE_PANEL_W))
            .h_full()
            .min_h(px(0.0))
            .overflow_hidden()
            .bg(bg)
            .border_l_1()
            .border_color(border)
            .child(
                v_flex()
                    .p_3()
                    .gap_2()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child("概览"),
                    )
                    .child(stat_row("条目总数", total.to_string(), fg, 0.55))
                    .child(stat_row("待改名", changed.to_string(), primary, 0.55))
                    .child(stat_row("有问题", issues.to_string(), danger, 0.55))
                    .child(stat_row("已改名", done.to_string(), fg, 0.55)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h(px(60.0))
                    .overflow_hidden()
                    .p_3()
                    .gap_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child("日志"),
                    )
                    .child(logs),
            )
            .child(
                v_flex()
                    .h(px(190.0))
                    .p_3()
                    .gap_1()
                    .border_t_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child(match self.method {
                                Method::Script => format!("{} 语法速查", self.lang.label()),
                                Method::Rule => "规则速查".to_string(),
                            }),
                    )
                    .child(
                        div()
                            .id("cheat-sheet")
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .text_size(px(10.0))
                            .line_height(px(15.0))
                            .text_color(muted)
                            .child(match self.method {
                                Method::Script => self.lang.cheat_sheet().to_string(),
                                Method::Rule => {
                                    let mut sheet =
                                        String::from("花括号里是变量，其余原样保留\n");
                                    for (var, desc, _) in model::RULE_VARS {
                                        sheet.push_str(&format!("{var}  {desc}\n"));
                                    }
                                    sheet.push_str("{seq:3}  补零到 3 位\n{{ }}  字面量花括号");
                                    sheet
                                }
                            }),
                    ),
            )
    }

    fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let color = match self.level {
            Level::Ok => cx.theme().primary,
            Level::Error => cx.theme().danger,
            Level::Info => muted,
        };
        h_flex()
            .w_full()
            .h(px(28.0))
            .px_4()
            .items_center()
            .justify_between()
            .bg(bg)
            .border_t_1()
            .border_color(border)
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(color)
                    .child(self.status.clone()),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(muted)
                    .child("应用脚本 = 只预览   开始处理 = 真正改名"),
            )
    }

    fn vars_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if !self.show_vars {
            return None;
        }
        let bg = cx.theme().background;
        let border = cx.theme().border;
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let primary = cx.theme().primary;
        let mono = cx.theme().mono_font_family.clone();

        let mut script_rows = v_flex().w_full().gap_1();
        for (name, desc, sample) in [
            (
                "index",
                "当前文件在列表里的下标，从 0 开始（第一个文件是 0，第二个是 1）。",
                "0",
            ),
            ("item.name", "文件名，含扩展名。文件夹就是文件夹名本身。", "photo.jpg"),
            ("item.ext", "扩展名，不含点；文件夹为空字符串。", "jpg"),
            ("item.stem", "不含扩展名的文件名。", "photo"),
            ("item.path", "完整路径，含文件名。", "C:\\pics\\photo.jpg"),
            ("item.dir", "文件所在文件夹的路径。", "C:\\pics"),
            ("item.size", "文件字节数；文件夹为 0。", "20480"),
            ("item.is_dir", "是不是文件夹，true / false。", "false"),
            (
                "item.createTime",
                "创建时间，本地时间；系统拿不到时为空字符串。",
                "2024-01-15 18:30:00",
            ),
            (
                "item.mediaDate",
                "媒体日期，可直接拼进文件名；不是图片 / 音频 / 视频时为空。",
                "20240115_183000",
            ),
            (
                "item.mediaType",
                "媒体类型：image / audio / video / document / other / folder。",
                "image",
            ),
            ("item.isImage", "是不是图片（按扩展名判断）。", "true"),
            ("item.isAudio", "是不是音频。", "false"),
            ("item.isVideo", "是不是视频。", "false"),
            ("item.isDocument", "是不是文档。", "false"),
        ] {
            script_rows = script_rows.child(var_line(name, desc, sample, fg, muted, mono.clone()));
        }

        let mut rule_rows = v_flex().w_full().gap_1();
        for (var, desc, sample) in model::RULE_VARS {
            rule_rows = rule_rows.child(var_line(var, desc, sample, fg, muted, mono.clone()));
        }

        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .bg(hsla(0.0, 0.0, 0.0, 0.35))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    v_flex()
                        .w(px(640.0))
                        .max_h(px(660.0))
                        .bg(bg)
                        .rounded_lg()
                        .border_1()
                        .border_color(border)
                        .p_5()
                        .gap_3()
                        .child(
                            h_flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(15.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(fg)
                                        .child("变量说明"),
                                )
                                .child(
                                    Button::new("vars-close")
                                        .ghost()
                                        .label("关闭")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.show_vars = false;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .id("vars-scroll")
                                .flex_1()
                                .min_h(px(0.0))
                                .overflow_y_scroll()
                                .child(
                                    v_flex()
                                        .gap_3()
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(primary)
                                                .child("自定义脚本：函数有两个参数"),
                                        )
                                        .child(script_rows)
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(primary)
                                                .child("自定义规则：花括号里的变量"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .line_height(px(16.0))
                                                .text_color(muted)
                                                .child(
                                                    "花括号外的字符原样保留。{seq:3} 表示补零到 3 位；\
                                                     要写字面量的花括号就写成 {{ 和 }}。",
                                                ),
                                        )
                                        .child(rule_rows),
                                ),
                        ),
                ),
        )
    }

    fn columns_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if !self.show_columns {
            return None;
        }
        let bg = cx.theme().background;
        let border = cx.theme().border;
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;

        let mut optional_rows = v_flex().gap_1();
        let mut locked_rows = v_flex().gap_1();
        for col in Col::all() {
            if col.locked() {
                locked_rows = locked_rows.child(
                    h_flex()
                        .w_full()
                        .py_1()
                        .items_center()
                        .justify_between()
                        .child(div().text_size(px(12.0)).text_color(fg).child(col.label()))
                        .child(div().text_size(px(10.0)).text_color(muted).child("固定显示")),
                );
                continue;
            }
            let on = self.columns.shows(col);
            optional_rows = optional_rows.child(
                v_flex()
                    .w_full()
                    .py_1()
                    .child(
                        Checkbox::new(SharedString::from(format!("col-{}", col.label())))
                            .label(col.label())
                            .checked(on)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.columns.toggle(col);
                                this.refresh_table(cx);
                            })),
                    ),
            );
        }

        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .bg(hsla(0.0, 0.0, 0.0, 0.35))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    v_flex()
                        .w(px(420.0))
                        .bg(bg)
                        .rounded_lg()
                        .border_1()
                        .border_color(border)
                        .p_5()
                        .gap_3()
                        .child(
                            h_flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(15.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(fg)
                                        .child("表格列设置"),
                                )
                                .child(
                                    Button::new("columns-close")
                                        .ghost()
                                        .label("关闭")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.show_columns = false;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .line_height(px(15.0))
                                .text_color(muted)
                                .child("勾选要显示的可选列。文件名、新文件名、路径、错误 是固定列，不能隐藏。"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child("可选列"),
                        )
                        .child(optional_rows)
                        .child(
                            div()
                                .w_full()
                                .h(px(1.0))
                                .bg(border),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child("固定列"),
                        )
                        .child(locked_rows)
                        .child(
                            div()
                                .w_full()
                                .h(px(1.0))
                                .bg(border),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .items_center()
                                .gap_3()
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .line_height(px(15.0))
                                                .text_color(muted)
                                                .child("拖动表头右边界调整列宽，双击边界按内容自适应"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .line_height(px(15.0))
                                                .text_color(muted)
                                                .child("列宽由表格自己维护，这里可以一键恢复默认"),
                                        ),
                                )
                                // 没有改过列宽就不显示这个按钮
                                .children(if self.widths.all_default() {
                                    None
                                } else {
                                    Some(
                                        Button::new("reset-widths")
                                            .outline()
                                            .label("恢复默认列宽")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.reset_all_widths(cx)
                                            })),
                                    )
                                }),
                        ),
                ),
        )
    }

    fn folder_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let folder = self.pending.first()?.clone();
        let bg = cx.theme().background;
        let border = cx.theme().border;
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let name = folder
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| folder.to_string_lossy().to_string());

        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .bg(hsla(0.0, 0.0, 0.0, 0.35))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    v_flex()
                        .w(px(500.0))
                        .bg(bg)
                        .rounded_lg()
                        .border_1()
                        .border_color(border)
                        .p_5()
                        .gap_3()
                        .child(
                            div()
                                .text_size(px(15.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child("这个文件夹要怎么处理？"),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .line_height(px(15.0))
                                .text_color(muted)
                                .child(folder.to_string_lossy().to_string()),
                        )
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    Checkbox::new("folder-recursive")
                                        .label("包含所有子文件夹里的文件")
                                        .checked(self.folder_recursive)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.folder_recursive = !this.folder_recursive;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.0))
                                        .line_height(px(15.0))
                                        .text_color(muted)
                                        .child(if self.folder_recursive {
                                            "会递归遍历每一层子文件夹，只收文件，不改文件夹名字"
                                        } else {
                                            "只处理这一层的文件，子文件夹会整个跳过"
                                        }),
                                ),
                        )
                        .child(
                            Button::new("folder-as-children")
                                .primary()
                                .w_full()
                                .label(if self.folder_recursive {
                                    format!("重命名「{name}」里的所有文件（含子文件夹）")
                                } else {
                                    format!("重命名「{name}」里的文件")
                                })
                                .on_click(cx.listener(|this, _, _, cx| this.resolve_pending(true, cx))),
                        )
                        .child(
                            Button::new("folder-as-self")
                                .outline()
                                .w_full()
                                .label(format!("重命名文件夹本身（{name}）"))
                                .on_click(cx.listener(|this, _, _, cx| this.resolve_pending(false, cx))),
                        )
                        .child(
                            Button::new("folder-skip")
                                .ghost()
                                .w_full()
                                .label("跳过这个文件夹")
                                .on_click(cx.listener(|this, _, _, cx| this.skip_pending(cx))),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(muted)
                                .child("一次拖入多个文件夹时，会依次询问每一个；上面的勾选对它们都生效。"),
                        ),
                ),
        )
    }
}


fn stat_row(label: impl Into<SharedString>, value: String, color: Hsla, label_alpha: f32) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(px(11.0))
                .text_color(color.opacity(label_alpha))
                .child(label.into()),
        )
        .child(div().text_size(px(13.0)).text_color(color).child(value))
}

/// 左侧规则面板里的一行：变量名 + 说明 + 示例。
fn rule_var_row(
    var: &str,
    desc: &str,
    sample: &str,
    fg: Hsla,
    muted: Hsla,
    mono: SharedString,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(88.0))
                .flex_shrink_0()
                .text_size(px(10.0))
                .font_family(mono)
                .text_color(fg)
                .child(var.to_string()),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(10.0))
                .text_color(muted)
                .overflow_hidden()
                .text_ellipsis()
                .child(desc.to_string()),
        )
        .child(
            div()
                .max_w(px(110.0))
                .text_size(px(10.0))
                .text_color(muted)
                .overflow_hidden()
                .text_ellipsis()
                .child(sample.to_string()),
        )
}

/// 变量说明弹窗里的一行。
fn var_line(
    name: &str,
    desc: &str,
    extra: &str,
    fg: Hsla,
    muted: Hsla,
    mono: SharedString,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_start()
        .gap_3()
        .child(
            div()
                .w(px(108.0))
                .flex_shrink_0()
                .text_size(px(11.0))
                .font_family(mono.clone())
                .text_color(fg)
                .child(name.to_string()),
        )
        .child(
            div()
                .flex_1()
                .text_size(px(11.0))
                .line_height(px(16.0))
                .text_color(muted)
                .child(desc.to_string()),
        )
        .child(
            div()
                .w(px(120.0))
                .flex_shrink_0()
                .text_size(px(10.0))
                .font_family(mono)
                .text_color(muted)
                .overflow_hidden()
                .text_ellipsis()
                .child(extra.to_string()),
        )
}


impl Render for RenameApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;
        let border = cx.theme().border;
        let entity = cx.entity();

        let mut root = v_flex()
            .size_full()
            .relative()
            .bg(bg)
            .child(self.toolbar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .items_stretch()
                    .child(self.method_panel(cx))
                    .child(self.table_panel(window, cx))
                    .child(div().w(px(1.0)).bg(border))
                    .child(self.side_panel(cx)),
            )
            .child(self.status_bar(cx));

        // 补全浮窗画在最上层（放在根节点末尾，避免被左栏的 overflow 裁掉）
        if let Some(popup) = self.completion_popup(&*window, cx) {
            root = root.child(popup);
        }

        if let Some(modal) = self.folder_modal(cx) {
            root = root.child(modal);
        }

        if let Some(modal) = self.vars_modal(cx) {
            root = root.child(modal);
        }

        if let Some(modal) = self.columns_modal(cx) {
            root = root.child(modal);
        }

        root.on_drop(move |paths: &ExternalPaths, _, cx| {
            let list: Vec<PathBuf> = paths.paths().to_vec();
            if list.is_empty() {
                return;
            }
            entity.update(cx, |this, cx| {
                this.add_paths(list);
                this.table_changed(cx);
            });
        })
    }
}
