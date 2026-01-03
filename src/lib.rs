//! 青空文庫形式をプレーンテキストに変換するライブラリ
//!
//! # 使用例
//!
//! ```
//! // 高レベルAPI
//! let input = "タイトル\n著者\n\n吾輩《わがはい》は猫である\n底本：青空文庫";
//! let plain = aozora2text::convert(input.as_bytes());
//! assert_eq!(plain, "吾輩は猫である\n");
//! ```
//!
//! ```
//! // 低レベルAPI
//! use aozora2text::{tokenizer::Tokenizer, extractor};
//!
//! let input = "吾輩《わがはい》は猫《ねこ》である";
//! let mut tokenizer = Tokenizer::new(input);
//! let tokens = tokenizer.tokenize();
//! let plain = extractor::extract(&tokens);
//! assert_eq!(plain, "吾輩は猫である");
//! ```

pub mod extractor;

// Re-export from aozora-core
pub use aozora_core::accent;
pub use aozora_core::document;
pub use aozora_core::encoding;
pub use aozora_core::gaiji;
pub use aozora_core::token;
pub use aozora_core::tokenizer;

/// 青空文庫形式のバイト列をプレーンテキストに変換
///
/// エンコーディング自動判定（UTF-8 / Shift_JIS）、
/// 本文抽出（前付け・後付け除去）を行う。
///
/// # Examples
///
/// ```
/// let input = "タイトル\n著者\n\n本文です\n底本：青空文庫";
/// let plain = aozora2text::convert(input.as_bytes());
/// assert_eq!(plain, "本文です\n");
/// ```
pub fn convert(input: &[u8]) -> String {
    let text = encoding::decode_to_utf8(input);
    let lines: Vec<&str> = text.lines().collect();
    let body_lines = document::extract_body_lines(&lines);

    let converted: Vec<String> = body_lines
        .iter()
        .map(|line| {
            let mut tokenizer = tokenizer::Tokenizer::new(line);
            let tokens = tokenizer.tokenize();
            extractor::extract(&tokens)
        })
        .collect();

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
/// let plain = aozora2text::convert_line(input);
/// assert_eq!(plain, "吾輩は猫である");
/// ```
pub fn convert_line(input: &str) -> String {
    let mut tokenizer = tokenizer::Tokenizer::new(input);
    let tokens = tokenizer.tokenize();
    extractor::extract(&tokens)
}
