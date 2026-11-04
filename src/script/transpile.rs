//! 把 JS / C++ / Python / Java / Rust 的函数体转译成 Rust。
//!
//! 这不是完整的语言实现，而是一个「足够用的表达式级转译器」：
//! 覆盖重命名脚本里 90% 的写法 —— 取字段、字符串拼接、模板串、
//! 大小写、替换、切分、截取、补零、if/else、返回字符串。

/// 脚本语言。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Js,
    Cpp,
    Python,
    Rust,
    Java,
}

impl Lang {
    pub fn all() -> [Lang; 5] {
        [Lang::Js, Lang::Cpp, Lang::Python, Lang::Rust, Lang::Java]
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::Js => "JavaScript",
            Lang::Cpp => "C++",
            Lang::Python => "Python",
            Lang::Rust => "Rust",
            Lang::Java => "Java",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Lang::Js => "JS",
            Lang::Cpp => "C++",
            Lang::Python => "PY",
            Lang::Rust => "RS",
            Lang::Java => "JAVA",
        }
    }

    /// 编辑器语法高亮用的语言标识。
    pub fn id(self) -> &'static str {
        match self {
            Lang::Js => "javascript",
            Lang::Cpp => "cpp",
            Lang::Python => "python",
            Lang::Rust => "rust",
            Lang::Java => "java",
        }
    }

    /// 从语言标识还原，未知的一律当 JS。
    pub fn from_id(id: &str) -> Lang {
        match id {
            "cpp" => Lang::Cpp,
            "python" => Lang::Python,
            "rust" => Lang::Rust,
            "java" => Lang::Java,
            _ => Lang::Js,
        }
    }

    /// 编辑器上方那行不可编辑的函数声明。
    pub fn signature(self) -> &'static str {
        match self {
            Lang::Js => "function (item, index) {",
            Lang::Cpp => "std::string rename(const Item& item, size_t index) {",
            Lang::Python => "def rename(item, index):",
            Lang::Rust => "fn rename(item: &Item, index: usize) -> String {",
            Lang::Java => "static String rename(Item item, int index) {",
        }
    }

    /// 编辑器下方那行不可编辑的收尾。Python 没有右花括号，留空占位。
    pub fn footer(self) -> &'static str {
        match self {
            Lang::Python => "",
            _ => "}",
        }
    }

    /// 编辑器里的默认脚本（都只用到被支持的语法子集）。
    pub fn template(self) -> &'static str {
        match self {
            Lang::Js => {
                r#"const seq = index + 1;
const padded = String(seq).padStart(3, "0");
return `${item.stem}_${padded}.${item.ext}`;"#
            }
            Lang::Cpp => {
                r#"return "IMG_" + std::to_string(index + 1) + "." + item.ext;"#
            }
            Lang::Python => {
                r#"if item.is_dir:
    return "DIR_" + item.name
else:
    return f"{item.stem}_{index + 1:03}.{item.ext}""#
            }
            Lang::Rust => {
                r#"let seq = index + 1;
format!("{:03}_{}.{}", seq, item.stem, item.ext)"#
            }
            Lang::Java => {
                r#"return String.format("%03d_%s.%s", index + 1, item.stem, item.ext);"#
            }
        }
    }

    /// 语法速查，显示在右侧面板。
    pub fn cheat_sheet(self) -> &'static str {
        match self {
            Lang::Js => {
                "item.name / .ext / .stem / .path / .dir / .size / .is_dir\n\
                 item.createTime / .mediaDate / .mediaType\n\
                 item.isImage / .isAudio / .isVideo / .isDocument\n\
                 item[\"name\"]  也等价于 item.name\n\
                 `模板${expr}`、\"a\" + b 拼接\n\
                 s.toUpperCase() s.toLowerCase() s.trim()\n\
                 s.replace(a, b) s.split(sep) s.slice(a, b)\n\
                 s.padStart(3, \"0\") s.length s.join(sep)\n\
                 String(x)  转字符串\n\
                 if (cond) { return x; } else { return y; }"
            }
            Lang::Cpp => {
                "item.name 等字段与 JS 相同\n\
                 item.createTime / .mediaDate / .mediaType\n\
                 item.isImage / .isAudio / .isVideo / .isDocument\n\
                 \"a\" + b 拼接，std::to_string(n) 转字符串\n\
                 s.substr(a, b)  s.size()  s.length()\n\
                 s.replace(a, b)? 请用 s.substr + 拼接\n\
                 if (cond) { return x; }"
            }
            Lang::Python => {
                "item.name 等字段，item[\"name\"] 也等价\n\
                 item.createTime / .mediaDate / .mediaType\n\
                 item.isImage / .isAudio / .isVideo / .isDocument\n\
                 f\"{expr}_{index:03d}\" 格式化字符串\n\
                 s.upper() s.lower() s.strip()\n\
                 s.replace(a, b) s.split(sep) s[a:b] s[:-4]\n\
                 s.zfill(3) len(s) str(n) int(s)\n\
                 if cond: ... elif ...: ... else: ...\n\
                 缩进即代码块，return 返回字符串"
            }
            Lang::Rust => {
                "item 是 Item 结构体，字段为 String / u64 / bool\n\
                 item.createTime / .mediaDate / .mediaType\n\
                 item.isImage / .isAudio / .isVideo / .isDocument\n\
                 format!(\"{:03}_{}\", index + 1, item.stem)\n\
                 原生字符串方法全部可用：replace / split / trim ...\n\
                 if / match / let 都可以用\n\
                 最后一个表达式即返回值（也可以写 return）"
            }
            Lang::Java => {
                "item.name 等字段与 JS 相同\n\
                 item.createTime / .mediaDate / .mediaType\n\
                 item.isImage / .isAudio / .isVideo / .isDocument\n\
                 String.format(\"%03d_%s.%s\", n, a, b)\n\
                 s.toUpperCase() s.toLowerCase() s.trim()\n\
                 s.replace(a, b) s.split(sep) s.substring(a, b)\n\
                 s.length() String.valueOf(n)\n\
                 if (cond) { return x; } else { return y; }"
            }
        }
    }
}

/// 入口：把用户写的函数体转成 Rust 语句块（会被包进 `-> String` 的闭包里）。
pub fn to_rust(lang: Lang, raw: &str) -> Result<String, String> {
    let normalized = raw.replace("\r\n", "\n").replace('\t', "    ");
    let body = extract_function_body(lang, &normalized);
    if body.trim().is_empty() {
        return Err("脚本内容为空".to_string());
    }

    let mut code = body.to_string();
    if lang == Lang::Python {
        code = python_to_blocks(&code);
    }
    code = convert_format_strings(&code, lang);
    code = convert_item_access(&code);
    code = convert_keywords(&code, lang);
    code = fold_calls(&code, lang);
    code = rewrite_string_concat(&code);
    code = ensure_return(&code, lang);
    Ok(code.trim().to_string())
}

// ---------------------------------------------------------------------------
// 词法小工具
// ---------------------------------------------------------------------------

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// i 指向引号，返回闭合引号之后的位置。
fn skip_string(cs: &[char], i: usize) -> usize {
    let quote = cs[i];
    // Rust 生字符串 r"..." / r#"..."# 不处理，交给编译器
    let mut j = i + 1;
    while j < cs.len() {
        if cs[j] == '\\' {
            j += 2;
            continue;
        }
        if cs[j] == quote {
            return j + 1;
        }
        j += 1;
    }
    cs.len()
}

/// 从 i（指向开括号）找到配对的闭括号，返回其位置。
fn find_matching(cs: &[char], i: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    let mut j = i;
    while j < cs.len() {
        let c = cs[j];
        if c == '"' || c == '\'' || c == '`' {
            j = skip_string(cs, j);
            continue;
        }
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(j);
            }
        }
        j += 1;
    }
    None
}

/// 把顶层（不在括号/字符串内）的 `sep` 切开。
fn split_top_level(s: &str, sep: char) -> Vec<String> {
    let cs: Vec<char> = s.chars().collect();
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c == '"' || c == '\'' || c == '`' {
            let end = skip_string(&cs, i);
            cur.extend(&cs[i..end]);
            i = end;
            continue;
        }
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if c == sep && depth == 0 {
            parts.push(cur.trim().to_string());
            cur = String::new();
        } else {
            cur.push(c);
        }
        i += 1;
    }
    if !cur.trim().is_empty() || !parts.is_empty() {
        parts.push(cur.trim().to_string());
    }
    parts
}

fn read_ident(cs: &[char], mut i: usize) -> (String, usize) {
    let start = i;
    while i < cs.len() && is_ident_char(cs[i]) {
        i += 1;
    }
    let mut name: String = cs[start..i].iter().collect();
    // 处理 std::to_string 这类路径
    while i + 1 < cs.len() && cs[i] == ':' && cs[i + 1] == ':' {
        let (next, j) = read_ident_simple(cs, i + 2);
        if next.is_empty() {
            break;
        }
        name.push_str("::");
        name.push_str(&next);
        i = j;
    }
    (name, i)
}

fn read_ident_simple(cs: &[char], mut i: usize) -> (String, usize) {
    let start = i;
    while i < cs.len() && is_ident_char(cs[i]) {
        i += 1;
    }
    (cs[start..i].iter().collect(), i)
}

/// 从 i（指向 `(`）读取参数原文，返回（参数原文, 闭括号之后的位置）。
fn read_args(cs: &[char], i: usize) -> (String, usize) {
    match find_matching(cs, i, '(', ')') {
        Some(end) => (cs[i + 1..end].iter().collect(), end + 1),
        None => (cs[i + 1..].iter().collect(), cs.len()),
    }
}

// ---------------------------------------------------------------------------
// 1. 允许用户直接粘贴整个函数定义
// ---------------------------------------------------------------------------

fn extract_function_body(lang: Lang, src: &str) -> String {
    let trimmed = src.trim();
    let has_header = (trimmed.starts_with("fn ")
        || trimmed.starts_with("function ")
        || trimmed.starts_with("def ")
        || trimmed.contains("String ")
        || trimmed.starts_with("let transform"))
        && trimmed.contains('(')
        && !trimmed.starts_with("return");

    if !has_header {
        return trimmed.to_string();
    }

    if lang == Lang::Python && trimmed.contains(':') && !trimmed.contains('{') {
        // def transform(item, index):
        //     ...
        let mut lines = trimmed.lines();
        lines.next();
        let body: Vec<&str> = lines.collect();
        return dedent(&body.join("\n"));
    }

    // 找到第一个 `{`，再找配对的 `}`，取中间内容
    let cs: Vec<char> = trimmed.chars().collect();
    if let Some(open) = cs.iter().position(|c| *c == '{') {
        if let Some(close) = find_matching(&cs, open, '{', '}') {
            let inner: String = cs[open + 1..close].iter().collect();
            return dedent(&inner);
        }
    }
    trimmed.to_string()
}

fn dedent(src: &str) -> String {
    let mut min = usize::MAX;
    for line in src.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.chars().take_while(|c| *c == ' ').count();
        min = min.min(indent);
    }
    if min == usize::MAX || min == 0 {
        return src.trim().to_string();
    }
    src.lines()
        .map(|line| {
            if line.len() >= min {
                line[min..].to_string()
            } else {
                line.trim_start().to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// 2. Python 缩进块 -> Rust 花括号
// ---------------------------------------------------------------------------

fn python_to_blocks(src: &str) -> String {
    let mut out = String::new();
    let mut stack: Vec<usize> = Vec::new();

    for raw_line in src.lines() {
        if raw_line.trim().is_empty() {
            continue;
        }
        let indent = raw_line.chars().take_while(|c| *c == ' ').count();
        let line = raw_line.trim();
        if let Some(comment) = line.strip_prefix('#') {
            // Python 的注释要变成 Rust 的注释
            out.push_str(&" ".repeat(indent));
            out.push_str("//");
            out.push_str(comment);
            out.push('\n');
            continue;
        }

        // 需要收拢多少个已经结束的代码块
        let mut close_count = 0usize;
        while let Some(&top) = stack.last() {
            if indent <= top {
                stack.pop();
                close_count += 1;
            } else {
                break;
            }
        }

        // `elif` / `else` 会和已经收拢的 `}` 合并成 `} else {`
        let is_else = line == "else:";
        let elif_cond = line
            .strip_prefix("elif ")
            .and_then(|rest| rest.strip_suffix(':'))
            .map(|c| c.trim().to_string());

        let mut stmt = line.to_string();
        let mut push_block = false;

        if is_else || elif_cond.is_some() {
            if close_count > 0 {
                close_count -= 1;
            }
            for _ in 0..close_count {
                out.push_str("}\n");
            }
            close_count = 0;
            stmt = match elif_cond {
                Some(cond) => format!("}} else if {} {{", cond),
                None => "} else {".to_string(),
            };
            push_block = true;
        } else if let Some(rest) = line.strip_prefix("if ") {
            if rest.ends_with(':') {
                stmt = format!("if {} {{", rest[..rest.len() - 1].trim());
                push_block = true;
            }
        } else if let Some(rest) = line.strip_prefix("for ") {
            if rest.ends_with(':') {
                stmt = format!("/* for 循环未支持: {} */ {{", rest);
                push_block = true;
            }
        } else if line == "break" || line == "continue" || line == "pass" {
            for _ in 0..close_count {
                out.push_str("}\n");
            }
            continue;
        } else if !stmt.ends_with('{') && !stmt.ends_with('}') && !stmt.ends_with(';') {
            stmt.push(';');
        }

        for _ in 0..close_count {
            out.push_str("}\n");
        }
        out.push_str(&" ".repeat(indent));
        out.push_str(&stmt);
        out.push('\n');
        if push_block {
            stack.push(indent);
        }
    }

    while stack.pop().is_some() {
        out.push_str("}\n");
    }
    out.trim().to_string()
}

// ---------------------------------------------------------------------------
// 3. 模板字符串 / f-string -> format!
// ---------------------------------------------------------------------------

fn convert_format_strings(src: &str, lang: Lang) -> String {
    match lang {
        Lang::Js => convert_template_literals(src),
        Lang::Python => convert_f_strings(src),
        _ => src.to_string(),
    }
}

fn convert_template_literals(src: &str) -> String {
    let cs: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c == '`' {
            // 读取到闭合反引号
            let mut j = i + 1;
            let mut parts: Vec<(String, bool)> = Vec::new(); // (内容, 是否表达式)
            let mut cur = String::new();
            while j < cs.len() && cs[j] != '`' {
                if cs[j] == '\\' && j + 1 < cs.len() {
                    cur.push(cs[j]);
                    cur.push(cs[j + 1]);
                    j += 2;
                    continue;
                }
                if cs[j] == '$' && j + 1 < cs.len() && cs[j + 1] == '{' {
                    if let Some(end) = find_matching(&cs, j + 1, '{', '}') {
                        parts.push((cur.clone(), false));
                        cur.clear();
                        let expr: String = cs[j + 2..end].iter().collect();
                        parts.push((expr, true));
                        j = end + 1;
                        continue;
                    }
                }
                cur.push(cs[j]);
                j += 1;
            }
            parts.push((cur, false));

            let mut fmt = String::new();
            let mut args = Vec::new();
            for (text, is_expr) in &parts {
                if *is_expr {
                    fmt.push_str("{}");
                    args.push(text.clone());
                } else {
                    fmt.push_str(&escape_for_format(text));
                }
            }
            out.push_str("format!(\"");
            out.push_str(&fmt);
            out.push('"');
            for a in args {
                out.push_str(", ");
                out.push_str(&a);
            }
            out.push(')');
            i = j + 1;
            continue;
        }
        if c == '"' || c == '\'' {
            let end = skip_string(&cs, i);
            out.extend(&cs[i..end]);
            i = end;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn escape_for_format(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '{' => out.push_str("{{"),
            '}' => out.push_str("}}"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out
}

fn convert_f_strings(src: &str) -> String {
    let cs: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        let is_f = c == 'f'
            && i + 1 < cs.len()
            && (cs[i + 1] == '"' || cs[i + 1] == '\'')
            && (i == 0 || !is_ident_char(cs[i - 1]));
        if is_f {
            let quote = cs[i + 1];
            let mut j = i + 2;
            let mut fmt = String::new();
            let mut args: Vec<String> = Vec::new();
            while j < cs.len() && cs[j] != quote {
                if cs[j] == '\\' && j + 1 < cs.len() {
                    fmt.push(cs[j]);
                    fmt.push(cs[j + 1]);
                    j += 2;
                    continue;
                }
                if cs[j] == '{' {
                    if j + 1 < cs.len() && cs[j + 1] == '{' {
                        fmt.push_str("{{");
                        j += 2;
                        continue;
                    }
                    if let Some(end) = find_matching(&cs, j, '{', '}') {
                        let inner: String = cs[j + 1..end].iter().collect();
                        let (expr, spec) = match inner.split_once(':') {
                            Some((e, s)) => (e.trim().to_string(), Some(s.trim().to_string())),
                            None => (inner.trim().to_string(), None),
                        };
                        fmt.push('{');
                        if let Some(spec) = spec {
                            // 03d -> :03 / 4s -> :4（Rust 的宽度语法兼容）
                            let spec = spec.trim_end_matches(|ch: char| ch.is_alphabetic());
                            if !spec.is_empty() && spec.chars().all(|ch| ch.is_ascii_digit()) {
                                fmt.push(':');
                                fmt.push_str(spec);
                            }
                        }
                        fmt.push('}');
                        args.push(expr);
                        j = end + 1;
                        continue;
                    }
                }
                if cs[j] == '}' && j + 1 < cs.len() && cs[j + 1] == '}' {
                    fmt.push_str("}}");
                    j += 2;
                    continue;
                }
                if cs[j] == '"' {
                    fmt.push_str("\\\"");
                    j += 1;
                    continue;
                }
                fmt.push(cs[j]);
                j += 1;
            }
            out.push_str("format!(\"");
            out.push_str(&fmt);
            out.push('"');
            for a in args {
                out.push_str(", ");
                out.push_str(&a);
            }
            out.push(')');
            i = j + 1;
            continue;
        }
        if c == '"' || c == '\'' {
            let end = skip_string(&cs, i);
            out.extend(&cs[i..end]);
            i = end;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// 4. item["name"] / item.get("name") -> item.name
// ---------------------------------------------------------------------------

const ITEM_FIELDS: [&str; 7] = ["name", "ext", "stem", "path", "dir", "size", "is_dir"];

fn convert_item_access(src: &str) -> String {
    let mut out = String::new();
    let cs: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        // item["x"]
        if cs[i] == 'i' && src_at(&cs, i, "item") {
            let j = i + 4;
            // item['x'] / item["x"]
            if j < cs.len() && cs[j] == '[' {
                if let Some(end) = find_matching(&cs, j, '[', ']') {
                    let key: String = cs[j + 1..end].iter().collect();
                    let key = key.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
                    if ITEM_FIELDS.contains(&key.as_str()) {
                        out.push_str("item.");
                        out.push_str(&key);
                        i = end + 1;
                        continue;
                    }
                }
            }
            // item.get("x")
            if j < cs.len() && cs[j] == '.' && src_at(&cs, j, ".get(") {
                if let Some(end) = find_matching(&cs, j + 4, '(', ')') {
                    let key: String = cs[j + 5..end].iter().collect();
                    let key = key.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
                    if ITEM_FIELDS.contains(&key.as_str()) {
                        out.push_str("item.");
                        out.push_str(&key);
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        out.push(cs[i]);
        i += 1;
    }
    out
}

fn src_at(cs: &[char], i: usize, pat: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    if i + p.len() > cs.len() {
        return false;
    }
    &cs[i..i + p.len()] == &p[..]
}

// ---------------------------------------------------------------------------
// 5. 关键字 / 字面量
// ---------------------------------------------------------------------------

fn convert_keywords(src: &str, lang: Lang) -> String {
    let mut s = src.to_string();
    match lang {
        Lang::Js => {
            s = s.replace("!==", "!=").replace("===", "==");
            s = replace_word(&s, "const", "let");
            s = replace_word(&s, "var", "let");
            s = replace_word(&s, "undefined", "\"\"");
        }
        Lang::Cpp | Lang::Java => {
            s = replace_word(&s, "nullptr", "\"\"");
            s = replace_word(&s, "null", "\"\"");
        }
        Lang::Python => {
            s = replace_word(&s, "True", "true");
            s = replace_word(&s, "False", "false");
            s = replace_word(&s, "and", "&&");
            s = replace_word(&s, "or", "||");
            s = replace_word(&s, "not", "!");
            s = replace_word(&s, "None", "\"\"");
        }
        Lang::Rust => {}
    }
    s
}

/// 按「单词」替换，跳过字符串字面量。
fn replace_word(src: &str, from: &str, to: &str) -> String {
    let cs: Vec<char> = src.chars().collect();
    let target: Vec<char> = from.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c == '"' || c == '\'' || c == '`' {
            let end = skip_string(&cs, i);
            out.extend(&cs[i..end]);
            i = end;
            continue;
        }
        if is_ident_start(c) && (i == 0 || !is_ident_char(cs[i - 1])) && src_at(&cs, i, from) {
            let after = i + target.len();
            let boundary = after >= cs.len() || !is_ident_char(cs[after]);
            if boundary {
                out.push_str(to);
                i = after;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// 6. 方法调用折叠：s.toUpperCase() -> to_upper(s)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    /// helper(base, args...)
    BaseFirst,
    /// helper(args..., base)
    ArgsFirst,
}

struct Rule {
    name: &'static str,
    helper: &'static str,
    order: Order,
    /// 「只有一个参数」时改用这个 helper（slice(x) -> sub_from）
    one_arg: Option<&'static str>,
    /// 追加在参数后面的默认参数（python zfill(3) -> pad_start(s, 3, "0")）
    extra: &'static [&'static str],
    /// 忽略参数（size() / length()）
    ignore_args: bool,
    /// 属性式（不带括号）：.length
    property: bool,
}

const fn rule(name: &'static str, helper: &'static str) -> Rule {
    Rule {
        name,
        helper,
        order: Order::BaseFirst,
        one_arg: None,
        extra: &[],
        ignore_args: false,
        property: false,
    }
}

const fn prop(name: &'static str, helper: &'static str) -> Rule {
    Rule {
        name,
        helper,
        order: Order::BaseFirst,
        one_arg: None,
        extra: &[],
        ignore_args: false,
        property: true,
    }
}

fn rules(lang: Lang) -> &'static [Rule] {
    const JS: &[Rule] = &[
        rule("toUpperCase", "to_upper"),
        rule("toLowerCase", "to_lower"),
        rule("trim", "trim_str"),
        rule("replace", "replace_all"),
        rule("replaceAll", "replace_all"),
        rule("split", "split"),
        rule("slice", "sub"),
        rule("substring", "sub"),
        rule("padStart", "pad_start"),
        rule("startsWith", "starts_with_h"),
        rule("endsWith", "ends_with_h"),
        rule("includes", "contains_h"),
        rule("join", "join"),
        prop("length", "len_str"),
        prop("size", "len_str"),
    ];
    const PY: &[Rule] = &[
        rule("upper", "to_upper"),
        rule("lower", "to_lower"),
        rule("strip", "trim_str"),
        rule("replace", "replace_all"),
        rule("split", "split"),
        Rule {
            name: "join",
            helper: "join",
            order: Order::ArgsFirst,
            one_arg: None,
            extra: &[],
            ignore_args: false,
            property: false,
        },
        Rule {
            name: "zfill",
            helper: "pad_start",
            order: Order::BaseFirst,
            one_arg: None,
            extra: &["\"0\""],
            ignore_args: false,
            property: false,
        },
        rule("startswith", "starts_with_h"),
        rule("endswith", "ends_with_h"),
        rule("find", "find_pos"),
    ];
    const JAVA: &[Rule] = &[
        rule("toUpperCase", "to_upper"),
        rule("toLowerCase", "to_lower"),
        rule("trim", "trim_str"),
        rule("replace", "replace_all"),
        rule("split", "split"),
        rule("substring", "sub"),
        rule("startsWith", "starts_with_h"),
        rule("endsWith", "ends_with_h"),
        rule("contains", "contains_h"),
        prop("length", "len_str"),
    ];
    const CPP: &[Rule] = &[
        rule("substr", "sub_len"),
        Rule {
            name: "size",
            helper: "len_str",
            order: Order::BaseFirst,
            one_arg: None,
            extra: &[],
            ignore_args: true,
            property: false,
        },
        Rule {
            name: "length",
            helper: "len_str",
            order: Order::BaseFirst,
            one_arg: None,
            extra: &[],
            ignore_args: true,
            property: true,
        },
        rule("append", "push_str_h"),
    ];
    match lang {
        Lang::Js => JS,
        Lang::Python => PY,
        Lang::Java => JAVA,
        Lang::Cpp => CPP,
        Lang::Rust => &[],
    }
}

fn find_rule(lang: Lang, name: &str) -> Option<&'static Rule> {
    rules(lang).iter().find(|r| r.name == name)
}

fn fold_calls(src: &str, lang: Lang) -> String {
    let cs: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c == '"' || c == '\'' || c == '`' {
            let end = skip_string(&cs, i);
            out.extend(&cs[i..end]);
            i = end;
            continue;
        }
        let prev_is_dot = i > 0 && (cs[i - 1] == '.' || cs[i - 1] == ':');
        if is_ident_start(c) && !prev_is_dot {
            if let Some((expr, next)) = parse_receiver(&cs, i, lang) {
                out.push_str(&expr);
                i = next.max(i + 1);
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn parse_receiver(cs: &[char], start: usize, lang: Lang) -> Option<(String, usize)> {
    let (mut name, mut i) = read_ident(cs, start);

    // 自由函数：String(x) / len(x) / str(x) / std::to_string(x)
    if i < cs.len() && cs[i] == '(' {
        if let Some(mapped) = free_call(lang, &name) {
            let (args, after) = read_args(cs, i);
            let args_rust = fold_calls(&args, lang);
            let expr = mapped(&args_rust);
            return Some(fold_chain(expr, cs, after, lang));
        }
        // 未知函数：原样保留，继续折叠其后的方法
        let (args, after) = read_args(cs, i);
        let base = format!("{}({})", name, fold_calls(&args, lang));
        return Some(fold_chain(base, cs, after, lang));
    }

    // 属性链：item.name / item.is_dir
    loop {
        if i + 1 < cs.len() && cs[i] == '.' && is_ident_start(cs[i + 1]) {
            let (next_name, j) = read_ident(cs, i + 1);
            if j < cs.len() && cs[j] == '(' {
                break; // 方法调用，交给 fold_chain
            }
            if let Some(r) = find_rule(lang, &next_name) {
                if r.property {
                    // .length 这类属性
                    name = apply_rule(r, &name, "");
                    i = j;
                    break;
                }
            }
            name.push('.');
            name.push_str(&next_name);
            i = j;
            continue;
        }
        break;
    }

    Some(fold_chain(name, cs, i, lang))
}

fn fold_chain(mut expr: String, cs: &[char], mut i: usize, lang: Lang) -> (String, usize) {
    while i + 1 < cs.len() && cs[i] == '.' && is_ident_start(cs[i + 1]) {
        let (name, j) = read_ident(cs, i + 1);
        if j < cs.len() && cs[j] == '(' {
            let (args, after) = read_args(cs, j);
            let args_rust = fold_calls(&args, lang);
            // 静态方法：String.format / String.valueOf / Integer.parseInt
            if expr == "String" && name == "format" {
                if let Some(f) = convert_java_format(&args_rust) {
                    expr = f;
                    i = after;
                    continue;
                }
            }
            if expr == "String" && name == "valueOf" {
                expr = to_string_call(&args_rust);
                i = after;
                continue;
            }
            if expr == "Integer" && name == "parseInt" {
                expr = to_int_call(&args_rust);
                i = after;
                continue;
            }
            match find_rule(lang, &name) {
                Some(r) => expr = apply_rule(r, &expr, &args_rust),
                None => {
                    expr.push('.');
                    expr.push_str(&name);
                    expr.push('(');
                    expr.push_str(&args_rust);
                    expr.push(')');
                }
            }
            i = after;
            continue;
        }
        match find_rule(lang, &name) {
            Some(r) if r.property => {
                expr = apply_rule(r, &expr, "");
                i = j;
                break;
            }
            _ => {
                expr.push('.');
                expr.push_str(&name);
                i = j;
            }
        }
    }
    (expr, i)
}

fn apply_rule(r: &Rule, base: &str, args: &str) -> String {
    if r.ignore_args || r.property {
        return format!("{}({})", r.helper, base);
    }
    let parts: Vec<String> = if args.trim().is_empty() {
        Vec::new()
    } else {
        split_top_level(args, ',')
    };

    if parts.len() == 1 {
        if let Some(alt) = r.one_arg {
            return format!("{}({}, {})", alt, base, parts[0]);
        }
    }

    let mut all: Vec<String> = Vec::new();
    match r.order {
        Order::BaseFirst => {
            all.push(base.to_string());
            all.extend(parts.iter().cloned());
        }
        Order::ArgsFirst => {
            all.extend(parts.iter().cloned());
            all.push(base.to_string());
        }
    }
    all.extend(r.extra.iter().map(|s| s.to_string()));

    // join 的参数需要引用
    if r.helper == "join" && r.order == Order::ArgsFirst {
        // python: sep.join(list) -> join(&list, sep)
        let sep = all.pop().unwrap_or_default();
        let list = all.pop().unwrap_or_default();
        return format!("join(&{}, {})", list, sep);
    }
    if r.helper == "join" {
        // js: list.join(sep) -> join(&list, sep)
        let base = all.remove(0);
        return format!("join(&{}, {})", base, all.join(", "));
    }

    format!("{}({})", r.helper, all.join(", "))
}

type Mapper = fn(&str) -> String;

fn free_call(lang: Lang, name: &str) -> Option<Mapper> {
    match (lang, name) {
        (Lang::Js, "String") | (Lang::Js, "String.valueOf") => Some(to_string_call),
        (Lang::Js, "parseInt") | (Lang::Js, "Number") => Some(to_int_call),
        (Lang::Python, "str") => Some(to_string_call),
        (Lang::Python, "int") => Some(to_int_call),
        (Lang::Python, "len") => Some(len_call),
        (Lang::Java, "String.valueOf") => Some(to_string_call),
        (Lang::Java, "Integer.parseInt") => Some(to_int_call),
        (Lang::Cpp, "std::to_string") => Some(to_string_call),
        (Lang::Cpp, "std::stoi") => Some(to_int_call),
        (Lang::Rust, "len") => Some(len_call),
        _ => None,
    }
}

fn to_string_call(args: &str) -> String {
    if is_simple_expr(args) {
        format!("{}.to_string()", args)
    } else {
        format!("({}).to_string()", args)
    }
}

fn to_int_call(args: &str) -> String {
    format!("to_int({})", args)
}

fn len_call(args: &str) -> String {
    format!("len_str({})", args)
}

/// `a`、`item.name`、`index + 1` 里只有前两种可以直接接 `.method()`。
fn is_simple_expr(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        && !s.starts_with('.')
        && !s.ends_with('.')
}

/// Java 的 `String.format("%03d_%s", a, b)` -> `format!("{:03}_{}", a, b)`
fn convert_java_format(args: &str) -> Option<String> {
    let parts = split_top_level(args, ',');
    let first = parts.first()?;
    if !first.starts_with('"') || !first.ends_with('"') || first.len() < 2 {
        return None;
    }
    let pattern = &first[1..first.len() - 1];
    let mut fmt = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '%' => {
                let mut spec = String::from("%");
                while let Some(&n) = chars.peek() {
                    if n.is_ascii_digit() || n == '0' || n == '-' || n == '.' || n == '+' {
                        spec.push(n);
                        chars.next();
                    } else {
                        break;
                    }
                }
                match chars.next() {
                    Some('s') | Some('d') | Some('f') | Some('S') => {
                        let width: String = spec.chars().skip(1).collect();
                        fmt.push('{');
                        if !width.is_empty() {
                            fmt.push(':');
                            fmt.push_str(&width);
                        }
                        fmt.push('}');
                    }
                    Some('%') => fmt.push('%'),
                    Some(other) => {
                        fmt.push_str(&spec);
                        fmt.push(other);
                    }
                    None => fmt.push_str(&spec),
                }
            }
            '{' => fmt.push_str("{{"),
            '}' => fmt.push_str("}}"),
            '"' => fmt.push_str("\\\""),
            _ => fmt.push(c),
        }
    }
    let rest: Vec<String> = parts.iter().skip(1).cloned().collect();
    Some(format!("format!(\"{}\", {})", fmt, rest.join(", ")))
}

// ---------------------------------------------------------------------------
// 7. 字符串拼接：a + b + c 且含字符串 -> format!
// ---------------------------------------------------------------------------

fn rewrite_string_concat(src: &str) -> String {
    let cs: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        if src_at(&cs, i, "return")
            && (i == 0 || !is_ident_char(cs[i - 1]))
            && (i + 6 >= cs.len() || !is_ident_char(cs[i + 6]))
        {
            // 找到表达式结尾
            let start = i + 6;
            let end = find_stmt_end(&cs, start);
            let expr: String = cs[start..end].iter().collect();
            if let Some(parts) = concat_parts(&expr) {
                out.push_str("return format!(\"");
                out.push_str(&vec!["{}"; parts.len()].join(""));
                out.push_str("\", ");
                out.push_str(&parts.join(", "));
                out.push(')');
            } else {
                out.push_str("return");
                out.push_str(&expr);
            }
            i = end;
            continue;
        }
        if cs[i] == '"' || cs[i] == '\'' || cs[i] == '`' {
            let end = skip_string(&cs, i);
            out.extend(&cs[i..end]);
            i = end;
            continue;
        }
        out.push(cs[i]);
        i += 1;
    }
    out
}

fn find_stmt_end(cs: &[char], start: usize) -> usize {
    let mut depth = 0i32;
    let mut j = start;
    while j < cs.len() {
        let c = cs[j];
        if c == '"' || c == '\'' || c == '`' {
            j = skip_string(cs, j);
            continue;
        }
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                if depth == 0 {
                    return j;
                }
                depth -= 1;
            }
            ';' if depth == 0 => return j,
            '\n' if depth == 0 => return j,
            _ => {}
        }
        j += 1;
    }
    cs.len()
}

fn concat_parts(expr: &str) -> Option<Vec<String>> {
    let parts = split_top_level(expr, '+');
    if parts.len() < 2 {
        return None;
    }
    let looks_stringy = parts.iter().any(|p| {
        (p.contains('"') || p.contains('\'') || p.starts_with("item.") || p.contains("item."))
            && !p.trim().is_empty()
    });
    if !looks_stringy {
        return None;
    }
    // `1 + 2` 这种纯数字表达式不要转
    if parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    Some(parts)
}

// ---------------------------------------------------------------------------
// 8. 保证返回值
// ---------------------------------------------------------------------------

fn ensure_return(src: &str, lang: Lang) -> String {
    let cs: Vec<char> = src.chars().collect();
    let has_return = contains_word(&cs, "return");
    let trimmed = src.trim_end();
    if has_return || trimmed.is_empty() {
        return format!("{trimmed}\n");
    }
    // Rust 允许尾表达式，其它语言需要显式 return
    if lang == Lang::Rust && !trimmed.ends_with(';') {
        return format!("{trimmed}\n");
    }

    let mut lines: Vec<String> = trimmed.lines().map(|l| l.to_string()).collect();
    while let Some(last) = lines.last() {
        if last.trim().is_empty() {
            lines.pop();
        } else {
            break;
        }
    }
    if let Some(last) = lines.pop() {
        let t = last.trim();
        if t.ends_with(';') || t.ends_with('}') || t.ends_with('{') {
            lines.push(last);
            return lines.join("\n") + "\n";
        }
        let indent: String = last.chars().take_while(|c| *c == ' ').collect();
        let stmt = if t.ends_with(';') {
            t.to_string()
        } else {
            format!("{t};")
        };
        lines.push(format!("{indent}return {stmt}"));
    }
    lines.join("\n") + "\n"
}

fn contains_word(cs: &[char], word: &str) -> bool {
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '"' || cs[i] == '\'' || cs[i] == '`' {
            i = skip_string(cs, i);
            continue;
        }
        if is_ident_start(cs[i]) && (i == 0 || !is_ident_char(cs[i - 1])) && src_at(cs, i, word) {
            let after = i + word.chars().count();
            if after >= cs.len() || !is_ident_char(cs[after]) {
                return true;
            }
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(lang: Lang, src: &str) -> String {
        to_rust(lang, src).unwrap()
    }

    #[test]
    fn js_template_and_pad() {
        let out = t(
            Lang::Js,
            "const seq = index + 1;\nconst p = String(seq).padStart(3, \"0\");\nreturn `${item.stem}_${p}.${item.ext}`;",
        );
        assert!(out.contains("format!(\"{}_{}.{}\", item.stem, p, item.ext)"), "{out}");
        assert!(out.contains("pad_start(seq.to_string(), 3, \"0\")"), "{out}");
        assert!(out.contains("let seq = index + 1;"), "{out}");
    }

    #[test]
    fn js_concat() {
        let out = t(Lang::Js, "return \"IMG_\" + std::to_string(index + 1) + \".\" + item.ext;");
        assert!(out.contains("format!("), "{out}");
    }

    #[test]
    fn python_blocks() {
        let out = t(
            Lang::Python,
            "if item.is_dir:\n    return \"DIR_\" + item.name\nelse:\n    return f\"{item.stem}_{index + 1:03}.{item.ext}\"",
        );
        assert!(out.contains("if item.is_dir {"), "{out}");
        assert!(out.contains("} else {"), "{out}");
        assert!(out.contains(":03"), "{out}");
    }

    #[test]
    fn python_methods() {
        let out = t(Lang::Python, "return item.name.upper()");
        assert_eq!(out.trim(), "return to_upper(item.name);");
    }

    #[test]
    fn java_format() {
        let out = t(
            Lang::Java,
            "return String.format(\"%03d_%s.%s\", index + 1, item.stem, item.ext);",
        );
        assert!(out.contains("format!(\"{:03}_{}.{}\""), "{out}");
    }

    #[test]
    fn cpp_to_string() {
        let out = t(
            Lang::Cpp,
            "return \"IMG_\" + std::to_string(index + 1) + \".\" + item.ext;",
        );
        assert!(out.contains("(index + 1).to_string()"), "{out}");
    }

    #[test]
    fn rust_passthrough() {
        let out = t(
            Lang::Rust,
            "format!(\"{:03}_{}.{}\", index + 1, item.stem, item.ext)",
        );
        assert!(out.contains("format!(\"{:03}_{}.{}\""), "{out}");
    }

    #[test]
    fn item_index_access() {
        let out = t(Lang::Js, "return item[\"name\"]");
        assert_eq!(out.trim(), "return item.name");
    }

    #[test]
    fn full_function_is_unwrapped() {
        let out = t(
            Lang::Js,
            "function transform(item, index) {\n  return item.name;\n}",
        );
        assert_eq!(out.trim(), "return item.name;");
    }

}
