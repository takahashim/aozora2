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

/// 句点コード（面-区-点）を抽出する。
///
/// 参照実装 kuten2png は `/[12]-\d{1,2}-\d{1,2}/` に一致する最初の部分文字列
/// だけを句点コードとして画像化する。面（先頭）は 1 か 2 の**1桁**、区・点は
/// それぞれ 1〜2桁。したがって面が無効な `24-1-3`（これは底本の位置参照で
/// あって句点コードではない）や `第4水準` の 4 などは一致せず、画像化されず
/// 注記のまま出る。従来は `\d+-\d+-\d+` を無条件に拾って `24-1-3` まで画像化
/// していたためオラクルと乖離していた。
fn extract_jis_code(description: &str) -> Option<String> {
    // 参照実装 kuten2png は説明に NON_0213_GAIJI = 「非0213外字」を含む場合、
    // たとえ [12]-\d{1,2}-\d{1,2} に見える部分（つくりの水準参照など）があっても
    // 画像化しない（注記のまま出す）。例:
    // ※［＃非0213外字：「厂＋菫」、ただし「菫」は第3水準1-92-16のつくりの形、…］
    if description.contains("非0213外字") {
        return None;
    }
    let chars: Vec<char> = description.chars().collect();
    let n = chars.len();
    for start in 0..n {
        // 面: [12]（1桁）。直後は '-'。
        if chars[start] != '1' && chars[start] != '2' {
            continue;
        }
        if start + 1 >= n || chars[start + 1] != '-' {
            continue;
        }
        // 区: \d{1,2} の直後に '-'（正規表現の貪欲一致＋バックトラックを再現。
        // 2桁を先に試し、ダメなら1桁）。
        let Some(ku_hyphen) = match_digits_then_hyphen(&chars, start + 2) else {
            continue;
        };
        // 点: \d{1,2}（貪欲に最大2桁、後続の制約なし）。1桁以上必須。
        let ten_start = ku_hyphen + 1;
        let mut j = ten_start;
        while j < n && j - ten_start < 2 && chars[j].is_ascii_digit() {
            j += 1;
        }
        if j == ten_start {
            continue;
        }
        return Some(chars[start..j].iter().collect());
    }

    None
}

/// `pos` から数字を貪欲に 1〜2 桁読み、その直後が '-' ならその '-' の位置を返す。
/// 参照実装の正規表現 `\d{1,2}-` のバックトラック（2桁優先、ダメなら1桁）を再現。
fn match_digits_then_hyphen(chars: &[char], pos: usize) -> Option<usize> {
    let n = chars.len();
    // 2桁 + '-'
    if pos + 2 < n
        && chars[pos].is_ascii_digit()
        && chars[pos + 1].is_ascii_digit()
        && chars[pos + 2] == '-'
    {
        return Some(pos + 2);
    }
    // 1桁 + '-'
    if pos + 1 < n && chars[pos].is_ascii_digit() && chars[pos + 1] == '-' {
        return Some(pos + 1);
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
    fn test_extract_jis_code_rejects_non0213() {
        // 「非0213外字」を含む説明は、水準参照 1-92-16 があっても句点コードにしない
        // （参照実装 NON_0213_GAIJI ガード）。
        assert_eq!(
            extract_jis_code("非0213外字：「厂＋菫」、ただし「菫」は第3水準1-92-16のつくりの形、読みは「わづか」、286-下-24"),
            None
        );
    }

    #[test]
    fn test_extract_jis_code_rejects_invalid_men() {
        // 参照実装 kuten2png は /[12]-\d{1,2}-\d{1,2}/ にしか一致しないので、
        // 面が 1・2 以外（24 など）や3桁以上のものは句点コードにしない。
        // 24-1-3 は底本位置参照であって句点コードではない → 画像化されず注記のまま。
        assert_eq!(extract_jis_code("「未」の「二」に代えて「三」、24-1-3"), None);
        assert_eq!(extract_jis_code("説明、3-14-11"), None); // 面3は無効
        assert_eq!(extract_jis_code("説明、4-94-51"), None); // 面4は無効
        // 面が 1・2 の正しい句点コードは従来どおり拾う。
        assert_eq!(
            extract_jis_code("「にんべん＋憂」、第3水準1-14-11"),
            Some("1-14-11".to_string())
        );
        assert_eq!(
            extract_jis_code("説明、2-94-51"),
            Some("2-94-51".to_string())
        );
        // 区・点は1〜2桁。1桁でも拾う。
        assert_eq!(extract_jis_code("説明、1-2-3"), Some("1-2-3".to_string()));
        // 面が多桁でも内部に [12]-\d{1,2}-\d{1,2} を含まなければ不一致。
        assert_eq!(extract_jis_code("24-1-3"), None);
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
