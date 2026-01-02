//! 外字（JIS外文字）の変換

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// JISコード→Unicode変換テーブル（コンパイル時埋め込み）
static JIS2UCS: Lazy<HashMap<&'static str, char>> =
    Lazy::new(|| include!(concat!(env!("OUT_DIR"), "/jis2ucs_table.rs")));

/// 外字説明からUnicode文字に変換
///
/// # 変換優先順位
/// 1. Unicode直接指定 (U+XXXX)
/// 2. JISコード指定 (X-XX-XX) → テーブル参照
/// 3. 変換不能 → 〓（ゲタ記号）
///
/// # Examples
///
/// ```
/// use aozora2text::gaiji::convert_gaiji;
///
/// assert_eq!(convert_gaiji("「丸印」、U+25CB"), '○');
/// ```
pub fn convert_gaiji(description: &str) -> char {
    // 1. Unicode直接指定を探す
    if let Some(unicode_char) = extract_unicode(description) {
        return unicode_char;
    }

    // 2. JISコードを探す
    if let Some(jis_code) = extract_jis_code(description) {
        let normalized = normalize_jis_code(&jis_code);
        if let Some(&ch) = JIS2UCS.get(normalized.as_str()) {
            return ch;
        }
    }

    // 3. 変換不能
    '〓'
}

/// "U+XXXX" パターンからUnicode文字を抽出
fn extract_unicode(description: &str) -> Option<char> {
    // "U+XXXX" または "u+XXXX" を探す
    let description_upper = description.to_uppercase();

    if let Some(pos) = description_upper.find("U+") {
        let hex_start = pos + 2;
        let hex_end = description[hex_start..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .count()
            + hex_start;

        if hex_end > hex_start {
            let hex = &description[hex_start..hex_end];
            if let Ok(code) = u32::from_str_radix(hex, 16) {
                return char::from_u32(code);
            }
        }
    }

    None
}

/// "X-XX-XX" パターンからJISコードを抽出
fn extract_jis_code(description: &str) -> Option<String> {
    // JISコードのパターン: 数字-数字-数字
    // 例: 1-2-22, 2-14-75
    let mut chars = description.chars().peekable();
    let mut result = String::new();

    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            result.push(c);

            // 続く数字を読む
            while chars.peek().map_or(false, |c| c.is_ascii_digit()) {
                result.push(chars.next().unwrap());
            }

            // ハイフンが続くか確認
            if chars.peek() == Some(&'-') {
                result.push(chars.next().unwrap());

                // 2番目の数字
                while chars.peek().map_or(false, |c| c.is_ascii_digit()) {
                    result.push(chars.next().unwrap());
                }

                // 2番目のハイフン
                if chars.peek() == Some(&'-') {
                    result.push(chars.next().unwrap());

                    // 3番目の数字
                    while chars.peek().map_or(false, |c| c.is_ascii_digit()) {
                        result.push(chars.next().unwrap());
                    }

                    // パターン "X-XX-XX" が完成
                    if result.matches('-').count() == 2 {
                        return Some(result);
                    }
                }
            }

            result.clear();
        }
    }

    None
}

/// JISコードを正規化（区・点を2桁ゼロ埋め）
/// 例: "1-2-22" → "1-02-22"
fn normalize_jis_code(code: &str) -> String {
    let parts: Vec<&str> = code.split('-').collect();
    if parts.len() == 3 {
        format!(
            "{}-{:0>2}-{:0>2}",
            parts[0],
            parts[1],
            parts[2]
        )
    } else {
        code.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_unicode() {
        assert_eq!(extract_unicode("「丸印」、U+25CB"), Some('○'));
        assert_eq!(extract_unicode("U+3042"), Some('あ'));
        assert_eq!(extract_unicode("テスト"), None);
    }

    #[test]
    fn test_extract_jis_code() {
        assert_eq!(extract_jis_code("「二の字点」、1-2-22"), Some("1-2-22".to_string()));
        assert_eq!(extract_jis_code("「文字」、2-14-75"), Some("2-14-75".to_string()));
        assert_eq!(extract_jis_code("テスト"), None);
    }

    #[test]
    fn test_normalize_jis_code() {
        assert_eq!(normalize_jis_code("1-2-22"), "1-02-22");
        assert_eq!(normalize_jis_code("2-14-75"), "2-14-75");
        assert_eq!(normalize_jis_code("1-1-1"), "1-01-01");
    }

    #[test]
    fn test_convert_gaiji_unicode() {
        assert_eq!(convert_gaiji("「丸印」、U+25CB"), '○');
    }

    #[test]
    fn test_convert_gaiji_unknown() {
        assert_eq!(convert_gaiji("不明な外字"), '〓');
    }
}
