//! aozora-core: 青空文庫形式処理のコアライブラリ
//!
//! このクレートは青空文庫形式のテキストを処理するための基本機能を提供します：
//!
//! - トークナイザ（字句解析）
//! - パーサー（構文解析）
//! - 文字種別判定
//! - 外字（JIS外文字）変換
//! - アクセント記号変換
//! - 文書構造解析
//! - エンコーディング検出・変換
//!
//! # 基本的な使い方
//!
//! ```
//! use aozora_core::tokenizer::tokenize;
//! use aozora_core::token::Token;
//!
//! let tokens = tokenize("漢字《かんじ》");
//! assert_eq!(tokens.len(), 2);
//! ```
//!
//! # パーサーの使い方
//!
//! ```
//! use aozora_core::tokenizer::tokenize;
//! use aozora_core::parser::parse;
//! use aozora_core::node::Node;
//!
//! let tokens = tokenize("｜東京《とうきょう》");
//! let nodes = parse(&tokens);
//! assert_eq!(nodes.len(), 1);
//! ```
//!
//! # モジュール構成
//!
//! - `delimiters` - 青空文庫形式で使用されるデリミタ定数
//! - `token` - トークン型の定義
//! - `tokenizer` - 字句解析（トークナイザ）
//! - `node` - ASTノード型の定義
//! - `parser` - 構文解析（パーサー）
//! - `char_type` - 文字種別判定
//! - `gaiji` - 外字変換
//! - `accent` - アクセント記号変換
//! - `document` - 文書構造解析
//! - `encoding` - エンコーディング検出・変換
//! - `zip` - ZIPファイル処理

pub mod accent;
pub mod char_type;
pub mod delimiters;
pub mod document;
pub mod encoding;
pub mod gaiji;
pub mod jis_table;
pub mod node;
pub mod parser;
pub mod token;
pub mod tokenizer;
pub mod zip;

// Re-exports for convenience
pub use char_type::{CharType, CharTypeExt};
pub use delimiters::{
    ACCENT_BEGIN, ACCENT_END, ACCENT_MARKS, COMMAND_BEGIN, COMMAND_END, GAIJI_MARK, IGETA,
    RUBY_BEGIN, RUBY_END, RUBY_PREFIX,
};
pub use node::{
    BlockParams, BlockType, MidashiLevel, MidashiStyle, Node, RubyDirection, StyleType,
};
pub use parser::parse;
pub use token::Token;
pub use tokenizer::{tokenize, Tokenizer};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("吾輩《わがはい》は猫である");
        assert_eq!(tokens.len(), 3);
        assert!(matches!(&tokens[0], Token::Text(s) if s == "吾輩"));
        assert!(matches!(&tokens[1], Token::Ruby { .. }));
        assert!(matches!(&tokens[2], Token::Text(s) if s == "は猫である"));
    }

    #[test]
    fn test_gaiji_conversion() {
        use gaiji::convert_gaiji;
        assert_eq!(convert_gaiji("U+25CB"), "○");
    }

    #[test]
    fn test_accent_conversion() {
        use accent::convert_accent;
        assert_eq!(convert_accent("cafe'"), "café");
    }

    #[test]
    fn test_encoding_detection() {
        use encoding::decode_to_utf8;
        assert_eq!(decode_to_utf8("こんにちは".as_bytes()), "こんにちは");
    }
}
