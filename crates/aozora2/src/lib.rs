//! aozora2 - 青空文庫形式の変換ライブラリ
//!
//! このクレートは青空文庫形式のテキストを変換する機能を提供します。
//!
//! # 機能
//!
//! - `strip` - プレーンテキストへの変換（注記・ルビを除去）
//! - `html` - HTMLへの変換
//!
//! # 使用例
//!
//! ```
//! use aozora2::strip;
//!
//! let input = "吾輩《わがはい》は猫である";
//! let plain = strip::convert_line(input);
//! assert_eq!(plain, "吾輩は猫である");
//! ```

pub mod html;
pub mod strip;

// Re-export aozora-core for downstream crates
pub use aozora_core;
