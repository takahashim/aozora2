//! 注記関連コマンドの解析
//!
//! 注記付き範囲と傍記のパターンを解析します。

use super::command_parser::CommandResult;

/// 注記付き範囲パターンを解析
///
/// - `注記付き` → AnnotationRangeStart
/// - `左に注記付き` → LeftAnnotationRangeStart
/// - `「...」の注記付き終わり` → AnnotationRangeEnd
/// - `左に「...」の注記付き終わり` → LeftAnnotationRangeEnd
pub fn try_parse_annotation_range(content: &str) -> Option<CommandResult> {
    // 開始パターン
    if content == "注記付き" {
        return Some(CommandResult::AnnotationRangeStart);
    }
    if content == "左に注記付き" {
        return Some(CommandResult::LeftAnnotationRangeStart);
    }

    // 終了パターン: 「（銘々）」の注記付き終わり
    if content.ends_with("の注記付き終わり") {
        let rest = content.trim_end_matches("の注記付き終わり");

        // 左パターン: 左に「...」の注記付き終わり
        if let Some(rest) = rest.strip_prefix("左に") {
            if let Some(annotation) = extract_bracket_content(rest) {
                return Some(CommandResult::LeftAnnotationRangeEnd {
                    annotation: annotation.to_string(),
                });
            }
        }

        // 通常パターン: 「...」の注記付き終わり
        if let Some(annotation) = extract_bracket_content(rest) {
            return Some(CommandResult::AnnotationRangeEnd {
                annotation: annotation.to_string(),
            });
        }
    }

    None
}

/// 傍記パターンを解析
///
/// `「対象」に「注記」の傍記` 形式を解析します。
pub fn try_parse_side_note(content: &str) -> Option<CommandResult> {
    if !content.ends_with("の傍記") {
        return None;
    }

    let rest = content.trim_end_matches("の傍記");

    // 「対象」に「注記」 形式を解析
    let first_start = rest.find('「')?;
    let first_end = rest.find('」')?;
    if first_end <= first_start {
        return None;
    }

    let target = &rest[first_start + '「'.len_utf8()..first_end];

    // 「に「」パターンを探す
    let after_first = &rest[first_end + '」'.len_utf8()..];
    if !after_first.starts_with('に') {
        return None;
    }

    let annotation_part = after_first.trim_start_matches('に');
    let annotation = extract_bracket_content(annotation_part)?;

    Some(CommandResult::SideNote {
        target: target.to_string(),
        annotation: annotation.to_string(),
    })
}

/// 「...」の内容を抽出
fn extract_bracket_content(s: &str) -> Option<&str> {
    let start = s.find('「')?;
    let end = s.rfind('」')?;
    if end <= start {
        return None;
    }
    Some(&s[start + '「'.len_utf8()..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotation_range_start() {
        assert_eq!(
            try_parse_annotation_range("注記付き"),
            Some(CommandResult::AnnotationRangeStart)
        );
    }

    #[test]
    fn test_left_annotation_range_start() {
        assert_eq!(
            try_parse_annotation_range("左に注記付き"),
            Some(CommandResult::LeftAnnotationRangeStart)
        );
    }

    #[test]
    fn test_annotation_range_end() {
        assert_eq!(
            try_parse_annotation_range("「銘々」の注記付き終わり"),
            Some(CommandResult::AnnotationRangeEnd {
                annotation: "銘々".to_string()
            })
        );
    }

    #[test]
    fn test_left_annotation_range_end() {
        assert_eq!(
            try_parse_annotation_range("左に「注」の注記付き終わり"),
            Some(CommandResult::LeftAnnotationRangeEnd {
                annotation: "注".to_string()
            })
        );
    }

    #[test]
    fn test_side_note() {
        assert_eq!(
            try_parse_side_note("「工場」に「×」の傍記"),
            Some(CommandResult::SideNote {
                target: "工場".to_string(),
                annotation: "×".to_string()
            })
        );
    }

    #[test]
    fn test_not_side_note() {
        assert_eq!(try_parse_side_note("普通のテキスト"), None);
    }
}
