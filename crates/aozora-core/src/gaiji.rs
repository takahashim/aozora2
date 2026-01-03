//! 外字（JIS外文字）の変換

use crate::jis_table::{jis_to_unicode, normalize_jis_code};

/// 外字説明からUnicode文字列に変換
///
/// # 変換優先順位
/// 1. Unicode直接指定 (U+XXXX)
/// 2. JISコード指定 (X-XX-XX) → テーブル参照
/// 3. 変換不能 → 〓（ゲタ記号）
///
/// # Examples
///
/// ```
/// use aozora_core::gaiji::convert_gaiji;
///
/// assert_eq!(convert_gaiji("「丸印」、U+25CB"), "○");
/// ```
pub fn convert_gaiji(description: &str) -> String {
    // 1. Unicode直接指定を探す
    if let Some(unicode_char) = extract_unicode(description) {
        return unicode_char.to_string();
    }

    // 2. JISコードを探す
    if let Some(jis_code) = extract_jis_code(description) {
        if let Some(unicode) = jis_to_unicode(&jis_code) {
            return unicode;
        }
    }

    // 3. 変換不能
    "〓".to_string()
}

/// 外字変換の結果
#[derive(Debug, Clone, PartialEq)]
pub enum GaijiResult {
    /// Unicode文字に変換成功
    Unicode(String),
    /// JISコードからUnicodeに変換成功
    JisConverted {
        /// JISコード
        jis_code: String,
        /// 変換後のUnicode文字列
        unicode: String,
    },
    /// JISコードはあるが画像が必要
    JisImage {
        /// JISコード
        jis_code: String,
    },
    /// 変換不能
    Unconvertible,
}

/// 外字説明を解析して結果を返す（HTML変換用）
pub fn parse_gaiji(description: &str) -> GaijiResult {
    // 1. Unicode直接指定を探す
    if let Some(unicode_char) = extract_unicode(description) {
        return GaijiResult::Unicode(unicode_char.to_string());
    }

    // 2. JISコードを探す
    if let Some(jis_code) = extract_jis_code(description) {
        let normalized = normalize_jis_code(&jis_code);
        if let Some(unicode) = jis_to_unicode(&normalized) {
            return GaijiResult::JisConverted {
                jis_code: normalized,
                unicode,
            };
        }
        return GaijiResult::JisImage {
            jis_code: normalized,
        };
    }

    // 3. 変換不能
    GaijiResult::Unconvertible
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
            while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                result.push(chars.next().unwrap());
            }

            // ハイフンが続くか確認
            if chars.peek() == Some(&'-') {
                result.push(chars.next().unwrap());

                // 2番目の数字
                while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                    result.push(chars.next().unwrap());
                }

                // 2番目のハイフン
                if chars.peek() == Some(&'-') {
                    result.push(chars.next().unwrap());

                    // 3番目の数字
                    while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
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
        assert_eq!(
            extract_jis_code("「二の字点」、1-2-22"),
            Some("1-2-22".to_string())
        );
        assert_eq!(
            extract_jis_code("「文字」、2-14-75"),
            Some("2-14-75".to_string())
        );
        assert_eq!(extract_jis_code("テスト"), None);
    }

    #[test]
    fn test_convert_gaiji_unicode() {
        assert_eq!(convert_gaiji("「丸印」、U+25CB"), "○");
    }

    #[test]
    fn test_convert_gaiji_unknown() {
        assert_eq!(convert_gaiji("不明な外字"), "〓");
    }

    #[test]
    fn test_convert_gaiji_jis_multi_char() {
        // 1-05-87 = カ (U+30AB) + 半濁点 (U+309A) = カ゚
        assert_eq!(convert_gaiji("1-05-87"), "カ゚");
    }

    #[test]
    fn test_extract_jis_code_with_description() {
        assert_eq!(
            extract_jis_code("半濁点付き片仮名カ、1-05-87"),
            Some("1-05-87".to_string())
        );
    }

    #[test]
    fn test_convert_gaiji_with_full_description() {
        assert_eq!(convert_gaiji("半濁点付き片仮名カ、1-05-87"), "カ゚");
    }

    #[test]
    fn test_parse_gaiji_unicode() {
        assert_eq!(
            parse_gaiji("「丸印」、U+25CB"),
            GaijiResult::Unicode("○".to_string())
        );
    }

    #[test]
    fn test_parse_gaiji_jis() {
        match parse_gaiji("1-05-87") {
            GaijiResult::JisConverted { jis_code, unicode } => {
                assert_eq!(jis_code, "1-05-87");
                assert_eq!(unicode, "カ゚");
            }
            _ => panic!("Expected JisConverted"),
        }
    }
}
