//! 文件表格：把 gpui-component 的 `DataTable` 接到重命名列表上。
//!
//! 拖列宽、双击按内容自适应、横向滚动、可视区域裁剪都由 `DataTable` 自己做，
//! 这一层只回答三个问题：有几列、每列多宽、每一格显示什么。
//!
//! 数据是主界面拍给这里的一份快照：表格在自己的更新里读不到主界面
//! （主界面正被借走），所以宁可多存一份，也不跨实体取数据。

use gpui_kit::component::table::{Column, TableDelegate, TableState};
use gpui_kit::component::{ActiveTheme, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::app::RenameApp;
use crate::columns::{Col, Widths, MAX_COL_W};
use crate::model::{Entry, Origin};

/// 文件名前面那个 `FILE` / `DIR` 标签大约占多宽（含间距），自适应列宽时补上。
const TAG_W: f32 = 36.0;

/// 表格里的一行，从 `Entry` 拍下来的快照。
#[derive(Clone, Debug)]
pub struct Row {
    pub index: usize,
    pub name: String,
    pub new_name: String,
    pub dir: String,
    pub ext: String,
    pub size: String,
    pub error: String,
    pub selected: bool,
    pub is_dir: bool,
    pub tag: &'static str,
    pub changed: bool,
}

impl Row {
    /// 这一列要显示的文字。
    fn text(&self, col: Col) -> String {
        match col {
            Col::Name => self.name.clone(),
            Col::NewName => {
                if self.new_name.is_empty() {
                    "—".to_string()
                } else {
                    self.new_name.clone()
                }
            }
            Col::Ext => {
                if self.ext.is_empty() {
                    "—".to_string()
                } else {
                    self.ext.clone()
                }
            }
            Col::Index => self.index.to_string(),
            Col::Size => self.size.clone(),
            Col::Path => self.dir.clone(),
            Col::Error => self.error.clone(),
        }
    }
}

/// 把条目列表拍成表格行。
pub fn rows_of(entries: &[Entry]) -> Vec<Row> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| Row {
            index,
            name: entry.name.clone(),
            new_name: entry.new_name.clone(),
            dir: entry.dir.clone(),
            ext: entry.ext.clone(),
            size: entry.display_size(),
            error: entry.error.clone().unwrap_or_default(),
            selected: entry.selected,
            is_dir: entry.is_dir,
            tag: match entry.origin {
                Origin::FolderSelf => "DIR",
                Origin::FolderChild => {
                    if entry.is_dir {
                        "SUB"
                    } else {
                        "FILE"
                    }
                }
                Origin::File => "FILE",
            },
            changed: entry.has_change(),
        })
        .collect()
}

/// 表格的 delegate：拿一份行快照 + 列定义，点击行时回写主界面。
pub struct EntryTable {
    app: WeakEntity<RenameApp>,
    rows: Vec<Row>,
    cols: Vec<Col>,
    widths: Widths,
}

impl EntryTable {
    pub fn new(app: WeakEntity<RenameApp>, rows: Vec<Row>, cols: Vec<Col>, widths: Widths) -> Self {
        Self {
            app,
            rows,
            cols,
            widths,
        }
    }

    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.rows = rows;
    }

    pub fn set_columns(&mut self, cols: Vec<Col>, widths: Widths) {
        self.cols = cols;
        self.widths = widths;
    }

    fn col(&self, ix: usize) -> Option<Col> {
        self.cols.get(ix).copied()
    }
}

impl TableDelegate for EntryTable {
    fn columns_count(&self, _: &App) -> usize {
        self.cols.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let Some(col) = self.col(col_ix) else {
            return Column::new("", "");
        };
        Column::new(col.key(), col.label())
            .width(px(self.widths.get(col)))
            .min_width(px(col.min_width()))
            .max_width(px(MAX_COL_W))
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let label = self.col(col_ix).map(|col| col.label()).unwrap_or("");
        h_flex().h_full().min_w(px(0.0)).items_center().child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
    }

    /// 行本身：选中底色 + 点击切换选中。填充用的空行不挂点击。
    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let tr = div().id(("file-row", row_ix));
        let Some(row) = self.rows.get(row_ix) else {
            return tr;
        };
        let bg = if row.selected {
            cx.theme().secondary
        } else {
            cx.theme().background
        };
        let app = self.app.clone();
        tr.cursor_pointer().bg(bg).on_click(move |_, _, cx| {
            if let Some(app) = app.upgrade() {
                app.update(cx, |this, cx| this.toggle_row(row_ix, cx));
            }
        })
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let (Some(row), Some(col)) = (self.rows.get(row_ix), self.col(col_ix)) else {
            return div().into_any_element();
        };

        let muted = cx.theme().muted_foreground;
        let primary = cx.theme().primary;
        let danger = cx.theme().danger;
        let color = match col {
            Col::Name => cx.theme().foreground,
            Col::NewName => {
                if row.changed {
                    primary
                } else {
                    muted
                }
            }
            Col::Error => danger,
            _ => muted,
        };
        let text = div()
            .min_w(px(0.0))
            .text_size(px(col.text_size()))
            .text_color(color)
            .overflow_hidden()
            .text_ellipsis()
            .child(row.text(col));

        if col == Col::Name {
            let tag_color = if row.is_dir { primary } else { muted };
            return h_flex()
                .h_full()
                .min_w(px(0.0))
                .items_center()
                .gap_1()
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(10.0))
                        .text_color(tag_color)
                        .child(row.tag),
                )
                .child(text)
                .into_any_element();
        }

        h_flex()
            .h_full()
            .min_w(px(0.0))
            .items_center()
            .child(text)
            .into_any_element()
    }

    /// 双击列边界：把这一列调到刚好放下最宽的一条内容。
    fn fit_column(
        &mut self,
        col_ix: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Option<Pixels> {
        let col = self.col(col_ix)?;
        let font = window.text_style().font();
        let size = px(col.text_size());
        let color = cx.theme().foreground;

        let mut widest = px(0.0);
        let measure = |text: &str, widest: &mut Pixels| {
            if text.is_empty() {
                return;
            }
            let run = TextRun {
                len: text.len(),
                font: font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let width = window
                .text_system()
                .layout_line(text, size, std::slice::from_ref(&run), None)
                .width;
            if width > *widest {
                *widest = width;
            }
        };

        measure(col.label(), &mut widest);
        for row in &self.rows {
            measure(&row.text(col), &mut widest);
        }

        // 文件名列前面还有一截标签；再留一点点余量给表头的字号差异。
        Some(widest + px(if col == Col::Name { TAG_W } else { 2.0 }))
    }
}
