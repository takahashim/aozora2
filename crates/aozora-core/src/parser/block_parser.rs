//! ブロック開始/終了の解析
//!
//! 「ここから...」「ここで...終わり」形式のコマンドを解析します。

use crate::node::{BlockParams, BlockType, FontSizeType, MidashiLevel, MidashiStyle, StyleType};

use super::command_parser::CommandResult;
use super::utils::extract_number;

/// ブロック開始を解析
pub fn parse_block_start(content: &str) -> CommandResult {
    let content = content.trim_start_matches("ここから");

    // 参照実装 exec_block_start_command は割り注を扱わないので注記のまま出す
    // （インラインの ［＃割り注］…［＃割り注終わり］ だけが割り注になる）
    if content.contains("割り注") {
        return CommandResult::Note(format!("ここから{content}"));
    }
    let mut params = BlockParams::default();
    params.is_block = true; // ここから pattern is block-level

    // ぶら下げパターン: 「N字下げ、折り返してM字下げ」または「改行天付き、折り返してN字下げ」
    if content.contains("折り返して") {
        if let Some(result) = try_parse_burasage(content, &mut params) {
            return result;
        }
    }

    // 数字を抽出
    if let Some(width) = extract_number(content) {
        params.width = Some(width);
    }

    // 段階を抽出
    if content.contains("段階") {
        if let Some(size) = extract_number(content) {
            params.font_size = Some(size);
        }
    }

    // ブロックタイプを判定
    if let Some(block_type) = BlockType::from_command(content) {
        // 見出しの場合はレベルも設定
        if block_type == BlockType::Midashi {
            params.level = MidashiLevel::from_command(content);
        }
        CommandResult::BlockStart { block_type, params }
    } else {
        CommandResult::Note(format!("ここから{content}"))
    }
}

/// ぶら下げパターンを解析
fn try_parse_burasage(content: &str, params: &mut BlockParams) -> Option<CommandResult> {
    let parts: Vec<&str> = content.split("折り返して").collect();
    if parts.len() != 2 {
        return None;
    }

    let first_part = parts[0];
    let second_part = parts[1];

    // 折り返し幅を抽出
    if let Some(wrap_width) = extract_number(second_part) {
        params.wrap_width = Some(wrap_width);
    }

    // 最初の部分から字下げ幅を抽出
    if first_part.contains("天付き") {
        // 改行天付き: 最初の行は左端から
        params.width = Some(0);
    } else if let Some(width) = extract_number(first_part) {
        params.width = Some(width);
    }

    Some(CommandResult::BlockStart {
        block_type: BlockType::Burasage,
        params: params.clone(),
    })
}

/// ブロック終了を解析
pub fn parse_block_end(content: &str) -> CommandResult {
    // 参照実装は「ここで割り注終わり」も扱わないので注記のまま出す
    if content.contains("割り注") {
        return CommandResult::Note(content.to_string());
    }
    let content = content
        .trim_start_matches("ここで")
        .trim_end_matches("終わり");

    if let Some(block_type) = BlockType::from_command(content) {
        CommandResult::BlockEnd { block_type }
    } else {
        CommandResult::Note(format!("ここで{content}終わり"))
    }
}

/// インライン終了を解析
pub fn parse_inline_end(content: &str) -> CommandResult {
    let content = content.trim_end_matches("終わり");

    // 固定パターン
    match content {
        "縦中横" => return CommandResult::TcyEnd,
        "割り注" => return CommandResult::WarigakiEnd,
        "キャプション" => return CommandResult::CaptionEnd,
        _ => {}
    }

    // 装飾終了
    if let Some(style_type) = StyleType::from_command(content) {
        return CommandResult::StyleEnd { style_type };
    }

    // ブロック終了
    if let Some(block_type) = BlockType::from_command(content) {
        return CommandResult::BlockEnd { block_type };
    }

    CommandResult::Note(format!("{content}終わり"))
}

/// 行単位字下げを解析
pub fn try_parse_line_indent(content: &str) -> Option<CommandResult> {
    if !content.contains("字下げ") {
        return None;
    }

    let width = extract_number(content)?;
    Some(CommandResult::LineIndent { width })
}

/// 行単位地付き/地からを解析
pub fn try_parse_line_chitsuki(content: &str) -> Option<CommandResult> {
    // 参照実装の PAT_CHITSUKI は /(地付き|字上げ)(終わり)*$/ なので、
    // 「地から」に限らず「地より２字上げ」なども地付き扱いになる。
    let body = content.trim_end_matches("終わり");

    if body.ends_with("地付き") {
        return Some(CommandResult::LineChitsuki { width: 0 });
    }

    if body.ends_with("字上げ") {
        let width = extract_number(body).unwrap_or(0);
        return Some(CommandResult::LineChitsuki { width });
    }

    None
}

/// 見出し開始を解析（ブロック外の見出し）
pub fn try_parse_midashi_start(content: &str) -> Option<CommandResult> {
    let level = MidashiLevel::from_command(content)?;
    let style = MidashiStyle::from_command(content);
    let mut params = BlockParams::default();
    params.level = Some(level);
    params.midashi_style = Some(style);
    Some(CommandResult::BlockStart {
        block_type: BlockType::Midashi,
        params,
    })
}

/// インラインフォントサイズ開始を解析
pub fn try_parse_font_size_start(content: &str) -> Option<CommandResult> {
    let (size_type, level) = FontSizeType::from_command(content)?;
    let mut params = BlockParams::default();
    params.font_size = Some(level);
    Some(CommandResult::BlockStart {
        block_type: match size_type {
            FontSizeType::Dai => BlockType::FontDai,
            FontSizeType::Sho => BlockType::FontSho,
        },
        params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 参照実装は「ここから割り注」を扱わないので注記のまま出す。
    /// 割り注になるのはインラインの ［＃割り注］…［＃割り注終わり］ だけ。
    #[test]
    fn test_block_warichu_stays_a_note() {
        assert_eq!(
            parse_block_start("ここから割り注"),
            CommandResult::Note("ここから割り注".to_string())
        );
        assert_eq!(
            parse_block_end("ここで割り注終わり"),
            CommandResult::Note("ここで割り注終わり".to_string())
        );
        // インラインの終了は従来どおり割り注として扱う
        assert_eq!(parse_inline_end("割り注終わり"), CommandResult::WarigakiEnd);
    }

    /// 地付きは「地から」に限らず末尾が「字上げ」なら成立する
    #[test]
    fn test_line_chitsuki_accepts_any_jiage_suffix() {
        assert_eq!(
            try_parse_line_chitsuki("地より２字上げ"),
            Some(CommandResult::LineChitsuki { width: 2 })
        );
        assert_eq!(
            try_parse_line_chitsuki("地から3字上げ"),
            Some(CommandResult::LineChitsuki { width: 3 })
        );
        assert_eq!(
            try_parse_line_chitsuki("地付き"),
            Some(CommandResult::LineChitsuki { width: 0 })
        );
        assert_eq!(try_parse_line_chitsuki("ここから２字下げ"), None);
    }

    #[test]
    fn test_parse_block_start_jisage() {
        let result = parse_block_start("ここから2字下げ");
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
        let result = parse_block_end("ここで字下げ終わり");
        assert_eq!(
            result,
            CommandResult::BlockEnd {
                block_type: BlockType::Jisage,
            }
        );
    }

    #[test]
    fn test_parse_line_indent() {
        let result = try_parse_line_indent("3字下げ");
        assert_eq!(result, Some(CommandResult::LineIndent { width: 3 }));
    }

    #[test]
    fn test_parse_line_chitsuki() {
        let result = try_parse_line_chitsuki("地付き");
        assert_eq!(result, Some(CommandResult::LineChitsuki { width: 0 }));

        let result = try_parse_line_chitsuki("地から3字上げ");
        assert_eq!(result, Some(CommandResult::LineChitsuki { width: 3 }));
    }

    #[test]
    fn test_parse_inline_end() {
        assert_eq!(parse_inline_end("縦中横終わり"), CommandResult::TcyEnd);
        assert_eq!(parse_inline_end("割り注終わり"), CommandResult::WarigakiEnd);
    }
}
