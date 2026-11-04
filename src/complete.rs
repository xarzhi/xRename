//! `item.` 后面的自动补全：字段表 + 光标上下文解析。
//!
//! 这里只做纯文本计算，不碰 GUI，方便单独测试。

/// `item` 的一个子属性。
#[derive(Clone, Copy, Debug)]
pub struct Field {
    /// 属性名，也是补全后插入的文本
    pub name: &'static str,
    /// 类型，显示在候选项右侧
    pub ty: &'static str,
    /// 一句话说明
    pub desc: &'static str,
}

/// 脚本里 `item` 的全部字段。
pub const ITEM_FIELDS: &[Field] = &[
    Field {
        name: "name",
        ty: "string",
        desc: "文件名（含扩展名）",
    },
    Field {
        name: "stem",
        ty: "string",
        desc: "主名（去掉最后一个扩展名）",
    },
    Field {
        name: "ext",
        ty: "string",
        desc: "扩展名（不含点，保留原大小写）",
    },
    Field {
        name: "path",
        ty: "string",
        desc: "完整路径",
    },
    Field {
        name: "dir",
        ty: "string",
        desc: "所在目录",
    },
    Field {
        name: "size",
        ty: "number",
        desc: "大小（字节；文件夹为 0）",
    },
    Field {
        name: "is_dir",
        ty: "bool",
        desc: "是不是文件夹",
    },
    Field {
        name: "createTime",
        ty: "string",
        desc: "创建时间 2024-01-15 18:30:00",
    },
    Field {
        name: "mediaDate",
        ty: "string",
        desc: "媒体日期 20240115_183000",
    },
    Field {
        name: "mediaType",
        ty: "string",
        desc: "image/audio/video/document/other/folder",
    },
    Field {
        name: "isImage",
        ty: "bool",
        desc: "是不是图片",
    },
    Field {
        name: "isAudio",
        ty: "bool",
        desc: "是不是音频",
    },
    Field {
        name: "isVideo",
        ty: "bool",
        desc: "是不是视频",
    },
    Field {
        name: "isDocument",
        ty: "bool",
        desc: "是不是文档",
    },
];

/// 光标处解析出的补全上下文。
#[derive(Debug, PartialEq, Eq)]
pub struct Context {
    /// 待替换片段的起始字节位置（`item.` 之后）
    pub start: usize,
    /// 已经输入的前缀，用来过滤候选
    pub query: String,
}

/// 判断光标是不是落在 `item.<前缀>` 上；是就返回替换范围与前缀。
pub fn context_at(text: &str, cursor: usize) -> Option<Context> {
    let cursor = cursor.min(text.len());
    if !text.is_char_boundary(cursor) {
        return None;
    }
    let before = &text[..cursor];

    // 从光标往前扫，拿到正在输入的前缀（标识符片段）
    let mut start = before.len();
    for (i, ch) in before.char_indices().rev() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            start = i;
        } else {
            break;
        }
    }
    let query = &before[start..];

    // 前缀前面必须是 `.`
    let head = before[..start].strip_suffix('.')?;

    // 再往前拿到接收者，允许 `item` 或 `xxx.item`
    let mut recv_start = head.len();
    for (i, ch) in head.char_indices().rev() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            recv_start = i;
        } else {
            break;
        }
    }
    let receiver = &head[recv_start..];
    if receiver != "item" && !receiver.ends_with(".item") {
        return None;
    }

    Some(Context {
        start,
        query: query.to_string(),
    })
}

/// 按前缀过滤字段，空前缀返回全部。
pub fn filtered(query: &str) -> Vec<&'static Field> {
    let q = query.to_ascii_lowercase();
    ITEM_FIELDS
        .iter()
        .filter(|f| f.name.to_ascii_lowercase().starts_with(&q))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_after_dot() {
        let text = "return item.";
        let ctx = context_at(text, text.len()).expect("should trigger");
        assert_eq!(ctx.start, text.len());
        assert_eq!(ctx.query, "");
    }

    #[test]
    fn detects_partial_prefix() {
        let text = "return item.na";
        let ctx = context_at(text, text.len()).expect("should trigger");
        assert_eq!(ctx.start, text.len() - 2);
        assert_eq!(ctx.query, "na");
    }

    #[test]
    fn detects_in_middle_of_word() {
        let text = "return item.name;";
        let cursor = text.find("na").unwrap() + 1; // 光标在 n|a 之间
        let ctx = context_at(text, cursor).expect("should trigger");
        assert_eq!(ctx.start, cursor - 1);
        assert_eq!(ctx.query, "n");
    }

    #[test]
    fn ignores_other_receivers() {
        assert!(context_at("return myitem.", 13).is_none());
        assert!(context_at("return items.", 13).is_none());
        assert!(context_at("return item", 11).is_none());
        assert!(context_at("return itemx.na", 15).is_none());
    }

    #[test]
    fn allows_nested_receiver() {
        let text = "x.item.na";
        let ctx = context_at(text, text.len()).expect("should trigger");
        assert_eq!(ctx.query, "na");
    }

    #[test]
    fn filters_by_prefix() {
        assert_eq!(filtered("").len(), ITEM_FIELDS.len());
        let names: Vec<&str> = filtered("n").iter().map(|f| f.name).collect();
        assert_eq!(names, vec!["name"]);
        let names: Vec<&str> = filtered("s").iter().map(|f| f.name).collect();
        assert_eq!(names, vec!["stem", "size"]);
        assert!(filtered("zzz").is_empty());
    }
}
