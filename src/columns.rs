//! 表格的列定义、显隐开关与列宽。
//!
//! 这一层不碰界面，只管「哪些列显示、顺序是什么、各自多宽」，方便单独测试。

/// 表格的一列。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Col {
    Name,
    NewName,
    Ext,
    Index,
    Size,
    Path,
    Error,
}

impl Col {
    pub fn label(self) -> &'static str {
        match self {
            Col::Name => "文件名",
            Col::NewName => "新文件名",
            Col::Ext => "扩展名",
            Col::Index => "index",
            Col::Size => "大小",
            Col::Path => "路径",
            Col::Error => "错误",
        }
    }

    /// 这一列能不能被隐藏。
    pub fn locked(self) -> bool {
        matches!(self, Col::Name | Col::NewName | Col::Path | Col::Error)
    }

    /// 列的唯一标识，交给表格做列的 key。
    pub fn key(self) -> &'static str {
        match self {
            Col::Name => "name",
            Col::NewName => "new_name",
            Col::Ext => "ext",
            Col::Index => "index",
            Col::Size => "size",
            Col::Path => "path",
            Col::Error => "error",
        }
    }

    /// 列里的文字字号。
    pub fn text_size(self) -> f32 {
        match self {
            Col::Name | Col::NewName => 12.0,
            _ => 11.0,
        }
    }

    /// 默认宽度。宽度是整列的宽（含单元格内边距）。
    ///
    /// 七个列都显示时总和约 740px，正好等于默认窗口下表格面板的宽度，
    /// 这样默认状态不会出现横向滚动条；窗口变窄或拖宽了才滚动。
    pub fn default_width(self) -> f32 {
        match self {
            Col::Name => 158.0,
            Col::NewName => 158.0,
            Col::Ext => 56.0,
            Col::Index => 48.0,
            Col::Size => 72.0,
            Col::Path => 158.0,
            Col::Error => 88.0,
        }
    }

    /// 拖窄时的下限，再窄文字就没法看了。
    pub fn min_width(self) -> f32 {
        match self {
            Col::Name => 100.0,
            Col::NewName => 100.0,
            Col::Ext => 54.0,
            Col::Index => 46.0,
            Col::Size => 64.0,
            Col::Path => 110.0,
            Col::Error => 70.0,
        }
    }

    pub fn all() -> [Col; 7] {
        [
            Col::Name,
            Col::NewName,
            Col::Ext,
            Col::Index,
            Col::Size,
            Col::Path,
            Col::Error,
        ]
    }
}

/// 每一列当前的宽度（像素）。这是列宽的唯一的来源：
/// 表格拖拽结束时把新宽度写回来，重建表格时再按它渲染。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Widths {
    name: f32,
    new_name: f32,
    ext: f32,
    index: f32,
    size: f32,
    path: f32,
    error: f32,
}

impl Default for Widths {
    fn default() -> Self {
        Self {
            name: Col::Name.default_width(),
            new_name: Col::NewName.default_width(),
            ext: Col::Ext.default_width(),
            index: Col::Index.default_width(),
            size: Col::Size.default_width(),
            path: Col::Path.default_width(),
            error: Col::Error.default_width(),
        }
    }
}

impl Widths {
    pub fn get(&self, col: Col) -> f32 {
        match col {
            Col::Name => self.name,
            Col::NewName => self.new_name,
            Col::Ext => self.ext,
            Col::Index => self.index,
            Col::Size => self.size,
            Col::Path => self.path,
            Col::Error => self.error,
        }
    }

    /// 设宽度，自动夹在「该列最小宽度」和整体上限之间。
    pub fn set(&mut self, col: Col, width: f32) {
        let width = width.clamp(col.min_width(), MAX_COL_W);
        match col {
            Col::Name => self.name = width,
            Col::NewName => self.new_name = width,
            Col::Ext => self.ext = width,
            Col::Index => self.index = width,
            Col::Size => self.size = width,
            Col::Path => self.path = width,
            Col::Error => self.error = width,
        }
    }

    /// 所有列都回到默认宽度。
    pub fn reset_all(&mut self) {
        *self = Self::default();
    }

    /// 这一列是不是还是默认宽度。
    pub fn is_default(&self, col: Col) -> bool {
        (self.get(col) - col.default_width()).abs() < 0.5
    }

    /// 是不是全都没动过。
    pub fn all_default(&self) -> bool {
        Col::all().iter().all(|c| self.is_default(*c))
    }
}

/// 单列宽度上限。窗口再宽也不需要比这更宽。
pub const MAX_COL_W: f32 = 900.0;

/// 可选列的显隐开关。固定列不在里面，永远显示。
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Columns {
    pub ext: bool,
    pub index: bool,
    pub size: bool,
}

impl Default for Columns {
    fn default() -> Self {
        Self {
            ext: true,
            index: true,
            size: true,
        }
    }
}

impl Columns {
    pub fn shows(&self, col: Col) -> bool {
        match col {
            Col::Ext => self.ext,
            Col::Index => self.index,
            Col::Size => self.size,
            _ => true,
        }
    }

    /// 切换某一列；固定列点了没反应。
    pub fn toggle(&mut self, col: Col) {
        match col {
            Col::Ext => self.ext = !self.ext,
            Col::Index => self.index = !self.index,
            Col::Size => self.size = !self.size,
            _ => {}
        }
    }

    /// 被隐藏的列数。
    pub fn hidden(&self) -> usize {
        Col::all().iter().filter(|c| !c.locked() && !self.shows(**c)).count()
    }
}

/// 按当前开关排出要显示的列。
pub fn visible(cols: &Columns) -> Vec<Col> {
    Col::all().into_iter().filter(|c| cols.shows(*c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shows_every_column() {
        let cols = Columns::default();
        assert_eq!(visible(&cols).len(), 7);
        assert_eq!(cols.hidden(), 0);
    }

    #[test]
    fn optional_columns_can_be_hidden() {
        let mut cols = Columns::default();
        cols.toggle(Col::Ext);
        let shown = visible(&cols);
        assert_eq!(shown.len(), 6);
        assert!(!shown.contains(&Col::Ext));
        assert_eq!(cols.hidden(), 1);
    }

    #[test]
    fn locked_columns_never_disappear() {
        let mut cols = Columns::default();
        for col in [Col::Name, Col::NewName, Col::Path, Col::Error] {
            cols.toggle(col);
        }
        let shown = visible(&cols);
        for col in [Col::Name, Col::NewName, Col::Path, Col::Error] {
            assert!(shown.contains(&col), "{col:?} 被隐藏了");
        }
        assert_eq!(cols.hidden(), 0);
    }

    #[test]
    fn order_is_stable_whatever_is_hidden() {
        let mut cols = Columns::default();
        cols.toggle(Col::Index);
        assert_eq!(
            visible(&cols),
            vec![
                Col::Name,
                Col::NewName,
                Col::Ext,
                Col::Size,
                Col::Path,
                Col::Error
            ]
        );
    }

    #[test]
    fn widths_start_at_defaults() {
        let w = Widths::default();
        assert!(w.all_default());
        for col in Col::all() {
            assert!((w.get(col) - col.default_width()).abs() < 0.01);
        }
    }

    #[test]
    fn width_is_clamped_to_min_and_max() {
        let mut w = Widths::default();
        w.set(Col::Name, 5.0);
        assert_eq!(w.get(Col::Name), Col::Name.min_width());
        w.set(Col::Name, 5000.0);
        assert_eq!(w.get(Col::Name), MAX_COL_W);
        w.set(Col::Name, 220.0);
        assert_eq!(w.get(Col::Name), 220.0);
    }

    #[test]
    fn reset_all_restores_defaults() {
        let mut w = Widths::default();
        w.set(Col::Name, 300.0);
        w.set(Col::Error, 200.0);
        assert!(!w.all_default());
        w.reset_all();
        assert!(w.all_default());
        assert_eq!(w.get(Col::Name), Col::Name.default_width());
        assert_eq!(w.get(Col::Error), Col::Error.default_width());
    }

    #[test]
    fn path_column_is_sized_like_any_other() {
        let mut w = Widths::default();
        w.set(Col::Path, 300.0);
        assert_eq!(w.get(Col::Path), 300.0);
        assert!(!w.all_default());
        w.set(Col::Path, Col::Path.default_width());
        assert_eq!(w.get(Col::Path), Col::Path.default_width());
    }
}
