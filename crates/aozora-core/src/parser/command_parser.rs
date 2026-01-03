//! コマンド文字列の解析
//!
//! `［＃...］` 形式のコマンド内容を解析し、適切なノードまたはコマンド情報を返します。

use crate::node::{BlockParams, BlockType, MidashiLevel, MidashiStyle, StyleType};

/// コマンド解析結果
#[derive(Debug, Clone, PartialEq)]
pub enum CommandResult {
    /// 装飾コマンド（後方参照）
    Style {
        /// 対象テキスト
        target: String,
        /// 接続詞（に、は、の）
        connector: String,
        /// 装飾タイプ
        style_type: StyleType,
    },

    /// 見出しコマンド（後方参照）
    Midashi {
        /// 対象テキスト
        target: String,
        /// 見出しレベル
        level: MidashiLevel,
        /// 見出しスタイル
        style: MidashiStyle,
    },

    /// ブロック開始
    BlockStart {
        /// ブロックタイプ
        block_type: BlockType,
        /// パラメータ
        params: BlockParams,
    },

    /// ブロック終了
    BlockEnd {
        /// ブロックタイプ
        block_type: BlockType,
    },

    /// 行単位字下げ
    LineIndent {
        /// 字数
        width: u32,
    },

    /// 注記
    Note(String),

    /// 画像
    Image {
        /// ファイル名
        filename: String,
        /// 代替テキスト
        alt: String,
        /// 幅
        width: Option<u32>,
        /// 高さ
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

    /// 左ルビ指定
    LeftRuby {
        /// 対象テキスト
        target: String,
        /// ルビテキスト
        ruby: String,
    },

    /// 未知のコマンド
    Unknown(String),
}

/// コマンド文字列を解析
pub fn parse_command(content: &str) -> CommandResult {
    let content = content.trim();

    // 1. 後方参照パターン: 「対象」に/は/の 装飾
    if let Some(result) = try_parse_reference(content) {
        return result;
    }

    // 2. ブロック開始: ここから...
    if content.starts_with("ここから") {
        return parse_block_start(content);
    }

    // 3. ブロック終了: ここで...終わり
    if content.starts_with("ここで") && content.ends_with("終わり") {
        return parse_block_end(content);
    }

    // 4. インライン終了: ...終わり
    if content.ends_with("終わり") {
        return parse_inline_end(content);
    }

    // 5. 行単位字下げ: N字下げ
    if let Some(result) = try_parse_line_indent(content) {
        return result;
    }

    // 6. 返り点
    if content.starts_with("返り点") {
        return CommandResult::Note(content.to_string());
    }

    // 7. 訓点送り仮名
    if content.starts_with("訓点送り仮名") {
        return CommandResult::Note(content.to_string());
    }

    // 8. 画像
    if content.ends_with("入る") {
        if let Some(result) = try_parse_image(content) {
            return result;
        }
    }

    // 9. 縦中横
    if content == "縦中横" {
        return CommandResult::TcyStart;
    }

    // 10. 割り注
    if content == "割り注" {
        return CommandResult::WarigakiStart;
    }

    // その他は注記
    CommandResult::Note(content.to_string())
}

/// 後方参照パターンを解析
fn try_parse_reference(content: &str) -> Option<CommandResult> {
    // パターン: 「対象」に/は/の 装飾
    let start = content.find('「')?;
    let end = content.find('」')?;
    if end <= start {
        return None;
    }

    let target = &content[start + '「'.len_utf8()..end];
    let rest = &content[end + '」'.len_utf8()..];

    // 接続詞を探す
    let (connector, spec) = if let Some(pos) = rest.find('に') {
        ("に", &rest[pos + 'に'.len_utf8()..])
    } else if let Some(pos) = rest.find('は') {
        ("は", &rest[pos + 'は'.len_utf8()..])
    } else if rest.find('の').is_some() {
        // 「の左に」「のルビ」などのパターン
        if rest.contains("の左に") && rest.contains("のルビ") {
            // 左ルビパターン: 「親文字」の左に「ルビ」のルビ
            if let Some(result) = try_parse_left_ruby(target, rest) {
                return Some(result);
            }
        }
        ("の", &rest[rest.find('の').unwrap() + 'の'.len_utf8()..])
    } else {
        return None;
    };

    // 見出しかどうか
    if connector == "は" {
        if let Some(level) = MidashiLevel::from_command(spec) {
            let style = MidashiStyle::from_command(spec);
            return Some(CommandResult::Midashi {
                target: target.to_string(),
                level,
                style,
            });
        }
    }

    // 装飾タイプを取得
    if let Some(style_type) = StyleType::from_command(spec) {
        return Some(CommandResult::Style {
            target: target.to_string(),
            connector: connector.to_string(),
            style_type,
        });
    }

    None
}

/// 左ルビパターンを解析
fn try_parse_left_ruby(target: &str, rest: &str) -> Option<CommandResult> {
    // パターン: の左に「ルビ」のルビ
    let ruby_start = rest.find("「")?;
    let ruby_end = rest.rfind("」")?;
    if ruby_end <= ruby_start {
        return None;
    }

    let ruby = &rest[ruby_start + '「'.len_utf8()..ruby_end];
    Some(CommandResult::LeftRuby {
        target: target.to_string(),
        ruby: ruby.to_string(),
    })
}

/// ブロック開始を解析
fn parse_block_start(content: &str) -> CommandResult {
    let content = content.trim_start_matches("ここから");
    let mut params = BlockParams::default();

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

/// ブロック終了を解析
fn parse_block_end(content: &str) -> CommandResult {
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
fn parse_inline_end(content: &str) -> CommandResult {
    let content = content.trim_end_matches("終わり");

    if content == "縦中横" {
        return CommandResult::TcyEnd;
    }
    if content == "割り注" {
        return CommandResult::WarigakiEnd;
    }
    if let Some(block_type) = BlockType::from_command(content) {
        return CommandResult::BlockEnd { block_type };
    }

    CommandResult::Note(format!("{content}終わり"))
}

/// 行単位字下げを解析
fn try_parse_line_indent(content: &str) -> Option<CommandResult> {
    // パターン: N字下げ, この行N字下げ
    if !content.contains("字下げ") {
        return None;
    }

    let width = extract_number(content)?;
    Some(CommandResult::LineIndent { width })
}

/// 画像コマンドを解析
fn try_parse_image(content: &str) -> Option<CommandResult> {
    // パターン: （説明）（ファイル名、横N×縦M）入る
    let content = content.trim_end_matches("入る").trim();

    // 最初の括弧から説明を取得
    let alt_start = content.find('（')?;
    let alt_end = content.find('）')?;
    let alt = content[alt_start + '（'.len_utf8()..alt_end].to_string();

    // 2番目の括弧からファイル情報を取得
    let rest = &content[alt_end + '）'.len_utf8()..];
    let info_start = rest.find('（')?;
    let info_end = rest.find('）')?;
    let info = &rest[info_start + '（'.len_utf8()..info_end];

    // ファイル名とサイズを分離
    let parts: Vec<&str> = info.split('、').collect();
    let filename = parts.first()?.to_string();

    let mut width = None;
    let mut height = None;

    if parts.len() > 1 {
        let size_part = parts[1];
        // 横N×縦M パターン
        if let Some(w_pos) = size_part.find('横') {
            if let Some(x_pos) = size_part.find('×') {
                let w_str = &size_part[w_pos + '横'.len_utf8()..x_pos];
                width = w_str.parse().ok();
            }
        }
        if let Some(h_pos) = size_part.find('縦') {
            let h_str = &size_part[h_pos + '縦'.len_utf8()..];
            height = h_str
                .trim_end_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .ok();
        }
    }

    Some(CommandResult::Image {
        filename,
        alt,
        width,
        height,
    })
}

/// 文字列から数字を抽出（全角数字も対応）
fn extract_number(s: &str) -> Option<u32> {
    let digits: String = s
        .chars()
        .filter_map(|c| {
            if c.is_ascii_digit() {
                Some(c)
            } else if ('０'..='９').contains(&c) {
                // 全角数字をASCII数字に変換
                Some((c as u32 - '０' as u32 + '0' as u32) as u8 as char)
            } else {
                None
            }
        })
        .collect();
    digits.parse().ok()
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
    fn test_extract_number() {
        assert_eq!(extract_number("2字下げ"), Some(2));
        assert_eq!(extract_number("10字詰め"), Some(10));
        assert_eq!(extract_number("字下げ"), None);
    }

    #[test]
    fn test_extract_number_fullwidth() {
        // 全角数字のテスト
        assert_eq!(extract_number("２字下げ"), Some(2));
        assert_eq!(extract_number("３字下げ"), Some(3));
        assert_eq!(extract_number("１０字詰め"), Some(10));
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
}
