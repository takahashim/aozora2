//! プレーンテキスト変換（strip）
//!
//! 青空文庫形式のテキストからルビ・注記を除去してプレーンテキストに変換します。

use aozora_core::accent::convert_accent;
use aozora_core::document;
use aozora_core::encoding;
use aozora_core::gaiji::convert_gaiji;
use aozora_core::token::Token;
use aozora_core::tokenizer::Tokenizer;

/// 青空文庫形式のバイト列をプレーンテキストに変換
///
/// エンコーディング自動判定（UTF-8 / Shift_JIS）、
/// 本文抽出（前付け・後付け除去）を行う。
///
/// # Examples
///
/// ```
/// let input = "タイトル\n著者\n\n本文です\n底本：青空文庫";
/// let plain = aozora2::strip::convert(input.as_bytes());
/// assert_eq!(plain, "本文です\n");
/// ```
pub fn convert(input: &[u8]) -> String {
    let text = encoding::decode_to_utf8(input);
    let lines: Vec<&str> = text.lines().collect();
    let body_lines = document::extract_body_lines(&lines);

    let converted: Vec<String> = body_lines.iter().map(|line| convert_line(line)).collect();

    // 冒頭と末尾の空行を削除
    let start = converted.iter().position(|s| !s.is_empty()).unwrap_or(0);
    let end = converted
        .iter()
        .rposition(|s| !s.is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);

    if start >= end {
        String::new()
    } else {
        converted[start..end].join("\n") + "\n"
    }
}

/// 青空文庫形式の文字列をプレーンテキストに変換（本文抽出なし）
///
/// 前付け・後付けの除去を行わず、入力全体を変換する。
///
/// # Examples
///
/// ```
/// let input = "吾輩《わがはい》は猫《ねこ》である";
/// let plain = aozora2::strip::convert_line(input);
/// assert_eq!(plain, "吾輩は猫である");
/// ```
pub fn convert_line(input: &str) -> String {
    let mut tokenizer = Tokenizer::new(input);
    let tokens = tokenizer.tokenize();
    extract(&tokens)
}

/// トークン列をプレーンテキストに変換
fn extract(tokens: &[Token]) -> String {
    tokens.iter().map(extract_token).collect()
}

/// 単一トークンからテキストを抽出
fn extract_token(token: &Token) -> String {
    match token {
        // テキスト: そのまま出力
        Token::Text(s) => s.clone(),

        // 暗黙ルビ: 削除（親文字は直前のTextに含まれる）
        Token::Ruby { .. } => String::new(),

        // 明示ルビ: 親文字部分のみ抽出
        Token::PrefixedRuby { base_children, .. } => extract(base_children),

        // コマンド: 削除
        Token::Command { .. } => String::new(),

        // 外字: Unicode文字列に変換
        Token::Gaiji { description } => convert_gaiji(description),

        // アクセント: 内容を抽出してアクセント変換
        Token::Accent { children } => convert_accent(&extract(children)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        assert_eq!(convert_line("こんにちは"), "こんにちは");
    }

    #[test]
    fn test_ruby_removed() {
        assert_eq!(convert_line("漢字《かんじ》"), "漢字");
    }

    #[test]
    fn test_prefixed_ruby() {
        assert_eq!(convert_line("｜東京《とうきょう》"), "東京");
    }

    #[test]
    fn test_command_removed() {
        assert_eq!(convert_line("猫である［＃「である」に傍点］"), "猫である");
    }

    #[test]
    fn test_gaiji_unicode() {
        assert_eq!(convert_line("※［＃「丸印」、U+25CB］"), "○");
    }

    #[test]
    fn test_complex() {
        assert_eq!(
            convert_line("吾輩《わがはい》は猫《ねこ》である［＃「である」に傍点］"),
            "吾輩は猫である"
        );
    }

    #[test]
    fn test_accent_conversion() {
        assert_eq!(convert_line("〔cafe'〕"), "café");
    }

    #[test]
    fn test_convert_with_header_footer() {
        let input = "タイトル\n著者\n\n本文です\n底本：青空文庫";
        let plain = convert(input.as_bytes());
        assert_eq!(plain, "本文です\n");
    }
}
