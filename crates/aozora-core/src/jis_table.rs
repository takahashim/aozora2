//! JISコード→Unicode変換テーブル
//!
//! JIS X 0213の文字コードからUnicode文字列への変換テーブルを提供します。
//! このモジュールは `gaiji` と `accent` モジュールの両方から使用されます。

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// JISコード→Unicode変換テーブル（コンパイル時埋め込み）
/// 値は &str（複数文字の合成文字に対応、例: カ゚ = カ + 半濁点）
static JIS2UCS: Lazy<HashMap<&'static str, &'static str>> =
    Lazy::new(|| include!(concat!(env!("OUT_DIR"), "/jis2ucs_table.rs")));

/// JISコードからUnicode文字列に変換
///
/// # Arguments
/// * `jis_code` - JISコード（例: "1-02-22", "2-14-75"）
///
/// # Returns
/// 変換成功時はUnicode文字列、失敗時はNone
///
/// # Examples
///
/// ```
/// use aozora_core::jis_table::jis_to_unicode;
///
/// // 1-05-87 = カ (U+30AB) + 半濁点 (U+309A) = カ゚
/// assert_eq!(jis_to_unicode("1-05-87"), Some("カ゚".to_string()));
/// ```
pub fn jis_to_unicode(jis_code: &str) -> Option<String> {
    let normalized = normalize_jis_code(jis_code);
    JIS2UCS.get(normalized.as_str()).map(|&s| s.to_string())
}

/// 表に載っている Unicode 文字列を列挙する（順序は不定）。
///
/// 文字集合の所属判定（[`crate::charset`]）が「この文字は JIS X 0213 にあるか」を
/// 逆引きするために使う。表そのものを公開する代わりに値だけ渡すことで、
/// 逆引き用の集合の作り方を利用側に閉じ込める。
pub(crate) fn unicode_values() -> impl Iterator<Item = &'static str> {
    JIS2UCS.values().copied()
}

/// JISコードを正規化（区・点を2桁ゼロ埋め）
///
/// # Examples
///
/// ```
/// use aozora_core::jis_table::normalize_jis_code;
///
/// assert_eq!(normalize_jis_code("1-2-22"), "1-02-22");
/// assert_eq!(normalize_jis_code("2-14-75"), "2-14-75");
/// ```
pub fn normalize_jis_code(code: &str) -> String {
    let parts: Vec<&str> = code.split('-').collect();
    if parts.len() == 3 {
        format!("{}-{:0>2}-{:0>2}", parts[0], parts[1], parts[2])
    } else {
        code.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_jis_code() {
        assert_eq!(normalize_jis_code("1-2-22"), "1-02-22");
        assert_eq!(normalize_jis_code("2-14-75"), "2-14-75");
        assert_eq!(normalize_jis_code("1-1-1"), "1-01-01");
    }

    #[test]
    fn test_jis_to_unicode() {
        // 1-05-87 = カ゚ (カ + 半濁点)
        assert_eq!(jis_to_unicode("1-05-87"), Some("カ゚".to_string()));
    }

    #[test]
    fn test_jis_to_unicode_not_found() {
        assert_eq!(jis_to_unicode("99-99-99"), None);
    }
}
