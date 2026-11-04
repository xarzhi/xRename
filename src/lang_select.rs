//! 脚本语言的下拉选择器（gpui-component 的 `Select`）。

use gpui_kit::component::searchable_list::SearchableListItem;
use gpui_kit::SharedString;

use crate::script::Lang;

/// 下拉列表里的一项，值就是 `Lang` 本身。
#[derive(Clone)]
pub struct LangItem(pub Lang);

impl SearchableListItem for LangItem {
    type Value = Lang;

    fn title(&self) -> SharedString {
        self.0.label().into()
    }

    fn value(&self) -> &Lang {
        &self.0
    }

    /// 只有 5 项，下拉框不需要搜索框，但真搜起来按语言名匹配也合理。
    fn matches(&self, query: &str) -> bool {
        self.0.label().to_lowercase().contains(&query.to_lowercase())
    }
}

/// 下拉列表的全部选项，顺序跟 `Lang::all()` 一致。
pub fn items() -> Vec<LangItem> {
    Lang::all().iter().copied().map(LangItem).collect()
}

/// 某个语言在列表里的下标，用来初始化选中项。
pub fn index_of(lang: Lang) -> usize {
    Lang::all().iter().position(|l| *l == lang).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    //! 注意：不能用 `use super::*`，它会把 gpui 的 `#[test]` 宏带进来，
    //! 内置的 `#[test]` 会被解析成 `#[gpui::test]`，参数对不上。
    use super::{index_of, items};
    use crate::script::Lang;
    use gpui_kit::component::searchable_list::SearchableListItem;

    #[test]
    fn items_cover_every_language() {
        let list = items();
        assert_eq!(list.len(), Lang::all().len());
        for (i, lang) in Lang::all().iter().enumerate() {
            assert_eq!(list[i].0, *lang);
            assert_eq!(index_of(*lang), i);
        }
    }

    #[test]
    fn titles_are_not_empty() {
        for item in items() {
            assert!(!item.title().is_empty());
        }
    }
}
