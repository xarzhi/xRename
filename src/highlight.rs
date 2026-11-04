//! 脚本编辑器的语法高亮。
//!
//! 不引语法树，只做一次线性扫描：把函数体切成关键字、字符串、注释、数字、
//! 函数调用、属性访问，再交给编辑器按主题里的语义色上色。
//! 颜色名（`keyword` / `string` …）由 gpui 主题解析，浅色深色都跟着变。

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::base::input::{
    EditorState, FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter,
    InputHighlighterFactory, Rope,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::script::Lang;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Keyword,
    Str,
    Comment,
    Number,
    Function,
    Property,
    /// true / false / null / None …
    Literal,
    /// item / index 这两个隐式参数
    Variable,
}

impl Kind {
    fn theme_name(self) -> &'static str {
        match self {
            Kind::Keyword => "keyword",
            Kind::Str => "string",
            Kind::Comment => "comment",
            Kind::Number => "number",
            Kind::Function => "function",
            Kind::Property => "property",
            Kind::Literal => "boolean",
            Kind::Variable => "variable",
        }
    }
}

// ---------------------------------------------------------------------------
// 词法扫描
// ---------------------------------------------------------------------------

const JS_KEYWORDS: &[&str] = &[
    "const", "let", "var", "function", "return", "if", "else", "for", "while", "do", "switch",
    "case", "break", "continue", "new", "typeof", "instanceof", "in", "of", "this", "try", "catch",
    "finally", "throw", "class", "extends", "import", "export", "default", "async", "await",
    "delete", "void", "yield",
];

const CPP_KEYWORDS: &[&str] = &[
    "return", "if", "else", "for", "while", "do", "switch", "case", "break", "continue", "const",
    "constexpr", "auto", "static", "inline", "namespace", "using", "class", "struct", "public",
    "private", "protected", "template", "typename", "virtual", "new", "delete", "sizeof", "this",
    "operator", "std", "size_t", "void", "int", "long", "unsigned", "char", "bool", "double",
    "float",
];

const PY_KEYWORDS: &[&str] = &[
    "def", "return", "if", "elif", "else", "for", "while", "in", "not", "and", "or", "is",
    "import", "from", "as", "pass", "break", "continue", "lambda", "class", "try", "except",
    "finally", "with", "global", "nonlocal", "assert", "del", "yield", "raise", "str", "int",
    "len", "range",
];

const RUST_KEYWORDS: &[&str] = &[
    "let", "mut", "fn", "return", "if", "else", "match", "for", "while", "in", "loop", "break",
    "continue", "struct", "enum", "impl", "trait", "use", "pub", "mod", "as", "move", "ref",
    "where", "self", "Self", "crate", "super", "unsafe", "const", "static", "type", "dyn",
];

const JAVA_KEYWORDS: &[&str] = &[
    "return", "if", "else", "for", "while", "do", "switch", "case", "break", "continue", "new",
    "class", "interface", "extends", "implements", "public", "private", "protected", "static",
    "final", "void", "int", "long", "double", "float", "char", "boolean", "String", "this",
    "import", "package", "try", "catch", "finally", "throw", "instanceof",
];

fn keywords(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Js => JS_KEYWORDS,
        Lang::Cpp => CPP_KEYWORDS,
        Lang::Python => PY_KEYWORDS,
        Lang::Rust => RUST_KEYWORDS,
        Lang::Java => JAVA_KEYWORDS,
    }
}

fn literals(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Python => &["True", "False", "None"],
        Lang::Rust => &["true", "false", "None", "Some", "Ok", "Err"],
        Lang::Cpp => &["true", "false", "nullptr", "NULL"],
        _ => &["true", "false", "null", "undefined", "NaN"],
    }
}

fn is_quote(b: u8) -> bool {
    matches!(b, b'"' | b'\'')
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn next_significant(bytes: &[u8], mut i: usize) -> Option<u8> {
    while i < bytes.len() {
        if !bytes[i].is_ascii_whitespace() {
            return Some(bytes[i]);
        }
        i += 1;
    }
    None
}

fn comment_end(bytes: &[u8], start: usize, lang: Lang) -> Option<usize> {
    let b = *bytes.get(start)?;
    if lang == Lang::Python {
        if b == b'#' {
            let mut i = start;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            return Some(i);
        }
        return None;
    }
    if b != b'/' {
        return None;
    }
    match bytes.get(start + 1) {
        Some(b'/') => {
            let mut i = start + 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            Some(i)
        }
        Some(b'*') => {
            let mut i = start + 2;
            while i + 1 < bytes.len() {
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    return Some(i + 2);
                }
                i += 1;
            }
            Some(bytes.len())
        }
        _ => None,
    }
}

/// 从 `start` 开始如果能认出字符串（含 Python / Rust 的前缀），返回它的结束位置。
fn string_end(bytes: &[u8], start: usize, lang: Lang) -> Option<usize> {
    let mut i = start;

    // Rust 原生字符串：r"..." / r#"..."#
    if lang == Lang::Rust && bytes.get(i) == Some(&b'r') {
        let mut p = i + 1;
        let mut hashes = 0usize;
        while bytes.get(p) == Some(&b'#') {
            hashes += 1;
            p += 1;
        }
        if bytes.get(p) == Some(&b'"') {
            let mut q = p + 1;
            while q < bytes.len() {
                if bytes[q] == b'"' {
                    let closed = (0..hashes).all(|k| bytes.get(q + 1 + k) == Some(&b'#'));
                    if closed {
                        return Some((q + 1 + hashes).min(bytes.len()));
                    }
                }
                q += 1;
            }
            return Some(bytes.len());
        }
    }

    // Python 的前缀：f"..." / r"..." / rb"..." …
    if lang == Lang::Python {
        let mut p = i;
        let mut n = 0;
        while n < 2
            && p < bytes.len()
            && matches!(bytes[p], b'r' | b'R' | b'f' | b'F' | b'b' | b'B' | b'u' | b'U')
        {
            p += 1;
            n += 1;
        }
        if n > 0 && bytes.get(p).copied().is_some_and(is_quote) {
            i = p;
        }
    }

    if i >= bytes.len() || !(is_quote(bytes[i]) || (lang == Lang::Js && bytes[i] == b'`')) {
        return None;
    }
    let quote = bytes[i];

    // Python 三引号
    if lang == Lang::Python
        && bytes.get(i + 1) == Some(&quote)
        && bytes.get(i + 2) == Some(&quote)
    {
        let mut q = i + 3;
        while q < bytes.len() {
            if bytes[q] == b'\\' {
                q += 2;
                continue;
            }
            if bytes[q] == quote && bytes.get(q + 1) == Some(&quote) && bytes.get(q + 2) == Some(&quote)
            {
                return Some((q + 3).min(bytes.len()));
            }
            q += 1;
        }
        return Some(bytes.len());
    }

    // JS 模板串可以跨行，其余字符串碰到换行就收手，避免整段代码被吞掉
    let multiline = quote == b'`';
    let mut q = i + 1;
    while q < bytes.len() {
        let c = bytes[q];
        if c == b'\\' {
            q += 2;
            continue;
        }
        if c == quote {
            return Some(q + 1);
        }
        if !multiline && c == b'\n' {
            return Some(q);
        }
        q += 1;
    }
    Some(bytes.len())
}

/// 把一段代码切成带类型的高亮片段（按字节偏移，顺序且互不重叠）。
pub fn tokens(lang: Lang, text: &str) -> Vec<(Range<usize>, Kind)> {
    let bytes = text.as_bytes();
    let kw = keywords(lang);
    let lit = literals(lang);
    let mut out: Vec<(Range<usize>, Kind)> = Vec::new();
    let mut i = 0usize;
    // 上一个有意义的字符，用来判断标识符是不是属性访问
    let mut prev: Option<u8> = None;

    while i < bytes.len() {
        let b = bytes[i];

        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        if let Some(end) = comment_end(bytes, i, lang) {
            out.push((i..end, Kind::Comment));
            i = end;
            prev = None;
            continue;
        }

        if let Some(end) = string_end(bytes, i, lang) {
            out.push((i..end, Kind::Str));
            i = end;
            prev = None;
            continue;
        }

        if b.is_ascii_digit() {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric()
                    || bytes[i] == b'.'
                    || bytes[i] == b'_'
                    || ((bytes[i] == b'+' || bytes[i] == b'-')
                        && matches!(bytes[i - 1], b'e' | b'E')))
            {
                i += 1;
            }
            out.push((start..i, Kind::Number));
            prev = Some(bytes[i - 1]);
            continue;
        }

        if is_ident_start(b) {
            let start = i;
            while i < bytes.len() && is_ident_char(bytes[i]) {
                i += 1;
            }
            let word = &text[start..i];
            let kind = if kw.contains(&word) {
                Some(Kind::Keyword)
            } else if lit.contains(&word) {
                Some(Kind::Literal)
            } else if next_significant(bytes, i) == Some(b'(') {
                Some(Kind::Function)
            } else if prev == Some(b'.') {
                Some(Kind::Property)
            } else if word == "item" || word == "index" {
                Some(Kind::Variable)
            } else {
                None
            };
            if let Some(kind) = kind {
                out.push((start..i, kind));
            }
            prev = Some(bytes[i - 1]);
            continue;
        }

        prev = Some(b);
        i += 1;
    }

    out
}

// ---------------------------------------------------------------------------
// 接到 gpui 的代码编辑器上
// ---------------------------------------------------------------------------

pub struct ScriptHighlighter {
    lang: Lang,
    spans: Vec<(Range<usize>, Kind)>,
}

impl ScriptHighlighter {
    fn new(lang: Lang) -> Self {
        Self {
            lang,
            spans: Vec::new(),
        }
    }

    fn style_of(&self, kind: Kind, resolver: &dyn HighlightStyleResolver) -> HighlightStyle {
        resolver.style(kind.theme_name()).unwrap_or_default()
    }
}

impl InputHighlighter for ScriptHighlighter {
    fn language(&self) -> SharedString {
        SharedString::from(self.lang.id())
    }

    fn update(
        &mut self,
        _edit: Option<InputEdit>,
        text: &Rope,
        _folding: bool,
        _window: &mut Window,
        _cx: &mut Context<EditorState>,
    ) {
        // 脚本都很短，整篇重扫比做增量更省事
        let text = text.to_string();
        self.spans = tokens(self.lang, &text);
    }

    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut out: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
        let mut cursor = range.start;
        for (span, kind) in &self.spans {
            if span.end <= range.start || span.start >= range.end {
                continue;
            }
            let start = span.start.max(range.start);
            let end = span.end.min(range.end);
            if start > cursor {
                out.push((cursor..start, HighlightStyle::default()));
            }
            if end > start {
                out.push((start..end, self.style_of(*kind, resolver)));
            }
            cursor = end.max(cursor);
        }
        if cursor < range.end {
            out.push((cursor..range.end, HighlightStyle::default()));
        }
        out
    }

    fn fold_ranges(&self, _text: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}

/// 装进编辑器状态的高亮工厂：语言名一变就换一套关键字表。
pub fn factory() -> InputHighlighterFactory {
    Rc::new(|language: &str| {
        Some(Box::new(ScriptHighlighter::new(Lang::from_id(language))) as Box<dyn InputHighlighter>)
    })
}

#[cfg(test)]
mod tests {
    // 这里刻意不写 `use super::*`：上面的模块里 glob 导入了 gpui 的宏，
    // 会把内置的 `#[test]` 顶掉。
    use super::{Kind, ScriptHighlighter, tokens};
    use crate::script::Lang;
    use gpui_kit::HighlightStyle;
    use gpui_kit::base::input::{HighlightStyleResolver, InputHighlighter};

    /// 主题没有语法色时的兜底解析器。
    struct NoStyles;

    impl HighlightStyleResolver for NoStyles {
        fn style(&self, _: &str) -> Option<HighlightStyle> {
            None
        }
    }

    fn kinds(lang: Lang, text: &str) -> Vec<(String, Kind)> {
        tokens(lang, text)
            .into_iter()
            .map(|(r, k)| (text[r].to_string(), k))
            .collect()
    }

    #[test]
    fn js_pieces() {
        let text = "const seq = index + 1; const base = item.stem; // 序号\nreturn `${base}_${seq}.${item.ext}`;";
        let found = kinds(Lang::Js, text);
        assert!(found.contains(&("const".to_string(), Kind::Keyword)));
        assert!(found.contains(&("return".to_string(), Kind::Keyword)));
        assert!(found.contains(&("index".to_string(), Kind::Variable)));
        assert!(found.contains(&("stem".to_string(), Kind::Property)));
        assert!(found.contains(&("1".to_string(), Kind::Number)));
        assert!(
            found
                .iter()
                .any(|(t, k)| *k == Kind::Str && t.starts_with("`${base}")),
            "模板串没有被识别: {found:?}"
        );
        assert!(
            found
                .iter()
                .any(|(t, k)| *k == Kind::Comment && t.contains("序号")),
            "注释没有被识别: {found:?}"
        );
    }

    #[test]
    fn python_comments_and_fstrings() {
        let found = kinds(Lang::Python, "if item.is_dir:\n    return f\"{item.stem}_{index}\"");
        assert!(found.contains(&("if".to_string(), Kind::Keyword)));
        assert!(found.contains(&("return".to_string(), Kind::Keyword)));
        assert!(
            found
                .iter()
                .any(|(t, k)| *k == Kind::Str && t.contains("item.stem")),
            "{found:?}"
        );

        let hash = kinds(Lang::Python, "# 只是注释\nvalue");
        assert_eq!(hash[0].1, Kind::Comment);
    }

    #[test]
    fn rust_raw_string_is_one_span() {
        let text = r##"let s = r#"a"b"#; let n = 3;"##;
        let found = kinds(Lang::Rust, text);
        assert!(
            found.iter().any(|(t, k)| *k == Kind::Str && t.contains("a\"b")),
            "{found:?}"
        );
        assert!(found.contains(&("3".to_string(), Kind::Number)));
    }

    #[test]
    fn spans_are_ordered_and_inside_the_text() {
        for lang in Lang::all() {
            let text = lang.template();
            let mut last = 0;
            for (range, _) in tokens(lang, text) {
                assert!(range.start >= last, "{} 片段重叠", lang.label());
                assert!(range.end <= text.len(), "{} 片段越界", lang.label());
                assert!(text.is_char_boundary(range.start));
                assert!(text.is_char_boundary(range.end));
                last = range.end;
            }
        }
    }

    #[test]
    fn styles_cover_the_requested_range() {
        let text = "return item.name;";
        let hl = ScriptHighlighter {
            lang: Lang::Js,
            spans: tokens(Lang::Js, text),
        };
        let styles = hl.styles(&(0..text.len()), &NoStyles);
        let mut cursor = 0;
        for (range, _) in styles {
            assert_eq!(range.start, cursor);
            cursor = range.end;
        }
        assert_eq!(cursor, text.len());
    }
}
