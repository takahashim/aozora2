//! 青空文庫形式をプレーンテキストに変換するライブラリ
//!
//! # 使用例
//!
//! ```
//! use aozora2text::{tokenizer::Tokenizer, extractor::PlainTextExtractor};
//!
//! let input = "吾輩《わがはい》は猫《ねこ》である";
//! let mut tokenizer = Tokenizer::new(input);
//! let tokens = tokenizer.tokenize();
//! let plain = PlainTextExtractor::extract(&tokens);
//! assert_eq!(plain, "吾輩は猫である");
//! ```

pub mod document;
pub mod encoding;
pub mod extractor;
pub mod gaiji;
pub mod token;
pub mod tokenizer;
