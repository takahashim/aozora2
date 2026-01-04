//! コマンド文字列の解析
//!
//! `［＃...］` 形式のコマンド内容を解析し、適切なノードまたはコマンド情報を返します。

use crate::node::{BlockParams, BlockType, FontSizeType, MidashiLevel, MidashiStyle, StyleType};

use super::block_parser::{
    parse_block_end, parse_block_start, parse_inline_end, try_parse_font_size_start,
    try_parse_line_chitsuki, try_parse_line_indent, try_parse_midashi_start,
};
use super::content_parser::{is_kaeriten, try_parse_image, try_parse_okurigana};
use super::reference_parser::{try_parse_left_ruby, try_parse_reference};

/// コマンド解析結果
#[derive(Debug, Clone, PartialEq)]
pub enum CommandResult {
    /// 装飾コマンド（後方参照）
    Style {
        target: String,
        connector: String,
        style_type: StyleType,
    },

    /// 見出しコマンド（後方参照）
    Midashi {
        target: String,
        level: MidashiLevel,
        style: MidashiStyle,
    },

    /// フォントサイズコマンド（後方参照）
    FontSize {
        target: String,
        size_type: FontSizeType,
        level: u32,
    },

    /// ブロック開始
    BlockStart {
        block_type: BlockType,
        params: BlockParams,
    },

    /// ブロック終了
    BlockEnd { block_type: BlockType },

    /// 行単位字下げ
    LineIndent { width: u32 },

    /// 行単位地付き/地から
    LineChitsuki { width: u32 },

    /// 注記
    Note(String),

    /// 画像
    Image {
        filename: String,
        alt: String,
        width: Option<u32>,
        height: Option<u32>,
    },

    /// 返り点
    Kaeriten(String),

    /// 訓点送り仮名
    Okurigana(String),

    /// 縦中横開始
    TcyStart,

    /// 縦中横終了
    TcyEnd,

    /// 割り注開始
    WarigakiStart,

    /// 割り注終了
    WarigakiEnd,

    /// 装飾開始
    StyleStart { style_type: StyleType },

    /// 装飾終了
    StyleEnd { style_type: StyleType },

    /// 左ルビ指定
    LeftRuby { target: String, ruby: String },

    /// 注記ルビ（「対象」に「注記」の注記）
    AnnotationRuby { target: String, annotation: String },

    /// 縦中横（後方参照）
    InlineTcy { target: String },

    /// 罫囲み（後方参照）
    InlineKeigakomi { target: String },

    /// 横組み（後方参照）
    InlineYokogumi { target: String },

    /// キャプション（後方参照）
    InlineCaption { target: String },

    /// キャプション開始
    CaptionStart,

    /// キャプション終了
    CaptionEnd,

    /// 注記付き範囲開始
    AnnotationRangeStart,

    /// 左に注記付き範囲開始
    LeftAnnotationRangeStart,

    /// 注記付き範囲終了
    AnnotationRangeEnd { annotation: String },

    /// 左に注記付き範囲終了
    LeftAnnotationRangeEnd { annotation: String },

    /// 傍記（工場に「×」の傍記）
    SideNote { target: String, annotation: String },

    /// 未知のコマンド
    Unknown(String),
}

/// コマンド文字列を解析
pub fn parse_command(content: &str) -> CommandResult {
    let content = content.trim();

    // 1. 左ルビパターン（後方参照より先にチェック）
    if content.contains("の左に") && content.contains("のルビ") {
        if let Some(result) = try_parse_left_ruby(content) {
            return result;
        }
    }

    // 2. 後方参照パターン: 「対象」に/は/の 装飾
    if let Some(result) = try_parse_reference(content) {
        return result;
    }

    // 3. ブロック開始: ここから...
    if content.starts_with("ここから") {
        return parse_block_start(content);
    }

    // 4. ブロック終了: ここで...終わり
    if content.starts_with("ここで") && content.ends_with("終わり") {
        return parse_block_end(content);
    }

    // 5. 注記付き範囲パターン
    if let Some(result) = try_parse_annotation_range(content) {
        return result;
    }

    // 6. 傍記パターン（「対象」に「注記」の傍記）
    if let Some(result) = try_parse_side_note(content) {
        return result;
    }

    // 7. インライン終了: ...終わり
    if content.ends_with("終わり") {
        return parse_inline_end(content);
    }

    // 6. 行単位字下げ: N字下げ
    if let Some(result) = try_parse_line_indent(content) {
        return result;
    }

    // 7. 行単位地付き/地から
    if let Some(result) = try_parse_line_chitsuki(content) {
        return result;
    }

    // 8. 返り点
    if is_kaeriten(content) {
        return CommandResult::Kaeriten(content.to_string());
    }

    // 9. 訓点送り仮名
    if let Some(okurigana) = try_parse_okurigana(content) {
        return CommandResult::Okurigana(okurigana);
    }

    // 10. 訓点送り仮名（説明付き）
    if content.starts_with("訓点送り仮名") {
        return CommandResult::Note(content.to_string());
    }

    // 11. 画像
    if content.ends_with("入る") {
        if let Some(result) = try_parse_image(content) {
            return result;
        }
    }

    // 12. 縦中横
    if content == "縦中横" {
        return CommandResult::TcyStart;
    }

    // 13. 割り注
    if content == "割り注" {
        return CommandResult::WarigakiStart;
    }

    // 13.5. 罫囲み（インライン）
    if content == "罫囲み" {
        return CommandResult::BlockStart {
            block_type: BlockType::Keigakomi,
            params: BlockParams::default(),
        };
    }

    // 13.6. 横組み（インライン）
    if content == "横組み" {
        return CommandResult::BlockStart {
            block_type: BlockType::Yokogumi,
            params: BlockParams::default(),
        };
    }

    // 14. 装飾開始
    if let Some(style_type) = StyleType::from_command(content) {
        return CommandResult::StyleStart { style_type };
    }

    // 15. キャプション開始
    if content == "キャプション" {
        return CommandResult::CaptionStart;
    }

    // 16. 見出し開始
    if let Some(result) = try_parse_midashi_start(content) {
        return result;
    }

    // 17. インラインフォントサイズ開始
    if let Some(result) = try_parse_font_size_start(content) {
        return result;
    }

    // その他は注記
    CommandResult::Note(content.to_string())
}

/// 注記付き範囲パターンを解析
fn try_parse_annotation_range(content: &str) -> Option<CommandResult> {
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

/// 傍記パターンを解析（「対象」に「注記」の傍記）
fn try_parse_side_note(content: &str) -> Option<CommandResult> {
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
    fn test_parse_style_bouten() {
        let result = parse_command("「である」に傍点");
        assert_eq!(
            result,
            CommandResult::Style {
                target: "である".to_string(),
                connector: "に".to_string(),
                style_type: StyleType::SesameDot,
            }
        );
    }

    #[test]
    fn test_parse_style_bold() {
        let result = parse_command("「重要」に太字");
        assert_eq!(
            result,
            CommandResult::Style {
                target: "重要".to_string(),
                connector: "に".to_string(),
                style_type: StyleType::Bold,
            }
        );
    }

    #[test]
    fn test_parse_midashi() {
        let result = parse_command("「第一章」は大見出し");
        assert_eq!(
            result,
            CommandResult::Midashi {
                target: "第一章".to_string(),
                level: MidashiLevel::O,
                style: MidashiStyle::Normal,
            }
        );
    }

    #[test]
    fn test_parse_midashi_dogyo() {
        let result = parse_command("「一」は同行中見出し");
        assert_eq!(
            result,
            CommandResult::Midashi {
                target: "一".to_string(),
                level: MidashiLevel::Naka,
                style: MidashiStyle::Dogyo,
            }
        );
    }

    #[test]
    fn test_parse_block_start_jisage() {
        let result = parse_command("ここから2字下げ");
        assert_eq!(
            result,
            CommandResult::BlockStart {
                block_type: BlockType::Jisage,
                params: BlockParams {
                    width: Some(2),
                    is_block: true,
                    ..Default::default()
                },
            }
        );
    }

    #[test]
    fn test_parse_block_end() {
        let result = parse_command("ここで字下げ終わり");
        assert_eq!(
            result,
            CommandResult::BlockEnd {
                block_type: BlockType::Jisage,
            }
        );
    }

    #[test]
    fn test_parse_line_indent() {
        let result = parse_command("3字下げ");
        assert_eq!(result, CommandResult::LineIndent { width: 3 });
    }

    #[test]
    fn test_parse_tcy() {
        assert_eq!(parse_command("縦中横"), CommandResult::TcyStart);
        assert_eq!(parse_command("縦中横終わり"), CommandResult::TcyEnd);
    }

    #[test]
    fn test_parse_warigaki() {
        assert_eq!(parse_command("割り注"), CommandResult::WarigakiStart);
        assert_eq!(parse_command("割り注終わり"), CommandResult::WarigakiEnd);
    }

    #[test]
    fn test_parse_unknown() {
        let result = parse_command("改ページ");
        assert_eq!(result, CommandResult::Note("改ページ".to_string()));
    }

    #[test]
    fn test_parse_block_start_jisage_fullwidth() {
        let result = parse_command("ここから２字下げ");
        assert_eq!(
            result,
            CommandResult::BlockStart {
                block_type: BlockType::Jisage,
                params: BlockParams {
                    width: Some(2),
                    is_block: true,
                    ..Default::default()
                },
            }
        );
    }

    #[test]
    fn test_parse_line_indent_fullwidth() {
        let result = parse_command("３字下げ");
        assert_eq!(result, CommandResult::LineIndent { width: 3 });
    }

    #[test]
    fn test_parse_line_chitsuki() {
        let result = parse_command("地付き");
        assert_eq!(result, CommandResult::LineChitsuki { width: 0 });

        let result = parse_command("地から１字上げ");
        assert_eq!(result, CommandResult::LineChitsuki { width: 1 });

        let result = parse_command("地から3字上げ");
        assert_eq!(result, CommandResult::LineChitsuki { width: 3 });
    }
}
