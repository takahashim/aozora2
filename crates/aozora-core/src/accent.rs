//! アクセント分解記法の変換

use once_cell::sync::Lazy;
use std::collections::HashMap;

use crate::delimiters::ACCENT_MARKS;
use crate::jis_table::jis_to_unicode;

/// アクセントテーブル（基底文字+記号 → JISコード）
static ACCENT_TABLE: Lazy<HashMap<&'static str, &'static str>> =
    Lazy::new(|| include!(concat!(env!("OUT_DIR"), "/accent_table.rs")));

/// アクセント分解記法を変換
///
/// `cafe'` → `café` のように、基底文字+アクセント記号を
/// アクセント付き文字に変換する。
///
/// # Examples
///
/// ```
/// use aozora_core::accent::convert_accent;
///
/// assert_eq!(convert_accent("cafe'"), "café");
/// assert_eq!(convert_accent("A'"), "Á");
/// ```
pub fn convert_accent(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut result = String::new();
    let mut i = 0;

    while i < chars.len() {
        // 3文字のリガチャをチェック (例: "ae&" → æ)
        if i + 2 < chars.len() && is_accent_mark(chars[i + 2]) {
            let key = format!("{}{}{}", chars[i], chars[i + 1], chars[i + 2]);
            if let Some(converted) = lookup_accent(&key) {
                result.push_str(&converted);
                i += 3;
                continue;
            }
        }

        // 2文字のアクセントをチェック (例: "e'" → é)
        if i + 1 < chars.len() && is_accent_mark(chars[i + 1]) {
            let key = format!("{}{}", chars[i], chars[i + 1]);
            if let Some(converted) = lookup_accent(&key) {
                result.push_str(&converted);
                i += 2;
                continue;
            }
        }

        // マッチしない場合はそのまま出力
        result.push(chars[i]);
        i += 1;
    }

    result
}

/// 文字がアクセント記号かどうか
pub fn is_accent_mark(c: char) -> bool {
    ACCENT_MARKS.contains(&c)
}

/// アクセントテーブルを検索してUnicode文字を返す
fn lookup_accent(key: &str) -> Option<String> {
    ACCENT_TABLE
        .get(key)
        .and_then(|jis_code| jis_to_unicode(jis_code))
}

/// アクセント変換結果（1文字分）
#[derive(Debug, Clone, PartialEq)]
pub enum AccentPart {
    /// 通常のテキスト
    Text(String),
    /// アクセント文字
    Accent {
        /// JISコード
        jis_code: String,
        /// 文字名（説明）
        name: String,
        /// Unicode文字
        unicode: String,
    },
}

/// アクセント分解記法をパースしてJISコード情報を含む結果を返す
///
/// レンダラーが画像出力を選べるよう、JISコード情報を保持する。
pub fn parse_accent(input: &str) -> Vec<AccentPart> {
    let chars: Vec<char> = input.chars().collect();
    let mut result = Vec::new();
    let mut text_buffer = String::new();
    let mut i = 0;

    while i < chars.len() {
        // 3文字のリガチャをチェック (例: "ae&" → æ)
        if i + 2 < chars.len() && is_accent_mark(chars[i + 2]) {
            let key = format!("{}{}{}", chars[i], chars[i + 1], chars[i + 2]);
            if let Some((jis_code, unicode)) = lookup_accent_with_code(&key) {
                // バッファのテキストを先に出力
                if !text_buffer.is_empty() {
                    result.push(AccentPart::Text(std::mem::take(&mut text_buffer)));
                }
                result.push(AccentPart::Accent {
                    jis_code,
                    name: accent_name(&key),
                    unicode,
                });
                i += 3;
                continue;
            }
        }

        // 2文字のアクセントをチェック (例: "e'" → é)
        if i + 1 < chars.len() && is_accent_mark(chars[i + 1]) {
            let key = format!("{}{}", chars[i], chars[i + 1]);
            if let Some((jis_code, unicode)) = lookup_accent_with_code(&key) {
                // バッファのテキストを先に出力
                if !text_buffer.is_empty() {
                    result.push(AccentPart::Text(std::mem::take(&mut text_buffer)));
                }
                result.push(AccentPart::Accent {
                    jis_code,
                    name: accent_name(&key),
                    unicode,
                });
                i += 2;
                continue;
            }
        }

        // マッチしない場合はバッファに追加
        text_buffer.push(chars[i]);
        i += 1;
    }

    // 残りのテキストを出力
    if !text_buffer.is_empty() {
        result.push(AccentPart::Text(text_buffer));
    }

    result
}

/// アクセントテーブルを検索してJISコードとUnicode文字の両方を返す
fn lookup_accent_with_code(key: &str) -> Option<(String, String)> {
    ACCENT_TABLE.get(key).and_then(|jis_code| {
        jis_to_unicode(jis_code).map(|unicode| (jis_code.to_string(), unicode))
    })
}

/// アクセント記号のパターンから説明文字列を生成
fn accent_name(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() == 2 {
        let base = chars[0];
        let mark = chars[1];
        let mark_name = match mark {
            '\'' => "アキュートアクセント",
            '`' => "グレーブアクセント",
            '^' => "サーカムフレックスアクセント",
            ':' => "ダイエレシス",
            '~' => "チルダ",
            '_' => "マクロン",
            ',' => "セディラ",
            _ => "アクセント",
        };
        // 小文字のみ「小文字」サフィックス付き
        if base.is_lowercase() {
            format!("{}付き{}小文字", mark_name, base.to_uppercase())
        } else {
            format!("{}付き{}", mark_name, base)
        }
    } else if chars.len() == 3 {
        // リガチャ
        let upper = key.starts_with(|c: char| c.is_uppercase());
        let case = if upper { "大文字" } else { "小文字" };
        format!("リガチャ{}", case)
    } else {
        key.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_accent() {
        assert_eq!(convert_accent("e'"), "é");
        assert_eq!(convert_accent("a`"), "à");
        assert_eq!(convert_accent("u:"), "ü");
    }

    #[test]
    fn test_word_with_accent() {
        assert_eq!(convert_accent("cafe'"), "café");
        assert_eq!(convert_accent("nai:ve"), "naïve");
    }

    #[test]
    fn test_uppercase() {
        assert_eq!(convert_accent("A'"), "Á");
        assert_eq!(convert_accent("E`"), "È");
    }

    #[test]
    fn test_ligature() {
        assert_eq!(convert_accent("ae&"), "æ");
        assert_eq!(convert_accent("AE&"), "Æ");
    }

    #[test]
    fn test_no_accent() {
        assert_eq!(convert_accent("hello"), "hello");
        assert_eq!(convert_accent("test"), "test");
    }

    #[test]
    fn test_unknown_combination() {
        // 未知の組み合わせはそのまま
        assert_eq!(convert_accent("z'"), "z'");
    }

    #[test]
    fn test_is_accent_mark() {
        assert!(is_accent_mark('\''));
        assert!(is_accent_mark('`'));
        assert!(is_accent_mark('^'));
        assert!(!is_accent_mark('a'));
        assert!(!is_accent_mark('1'));
    }

    // 仕様書（10-accents.md）のテストケース

    #[test]
    fn test_spec_accent_marks() {
        // 仕様で定義されているアクセント記号
        assert!(is_accent_mark('\'')); // アキュート
        assert!(is_accent_mark('`')); // グレーブ
        assert!(is_accent_mark('^')); // サーカムフレックス
        assert!(is_accent_mark('~')); // チルダ
        assert!(is_accent_mark(':')); // ウムラウト
        assert!(is_accent_mark('_')); // マクロン
        assert!(is_accent_mark('&')); // リガチャ
        assert!(is_accent_mark(',')); // セディラ
        assert!(is_accent_mark('/')); // ストローク
        assert!(is_accent_mark('@')); // 逆転
    }

    #[test]
    fn test_spec_basic_accents() {
        // 仕様書の基本例
        assert_eq!(convert_accent("A`"), "À"); // グレーブ
        assert_eq!(convert_accent("A'"), "Á"); // アキュート
        assert_eq!(convert_accent("A^"), "Â"); // サーカムフレックス
        assert_eq!(convert_accent("A~"), "Ã"); // チルダ
        assert_eq!(convert_accent("A:"), "Ä"); // ウムラウト
        assert_eq!(convert_accent("A_"), "Ā"); // マクロン
    }

    #[test]
    fn test_spec_special_accents() {
        // セディラ
        assert_eq!(convert_accent("C,"), "Ç");
        assert_eq!(convert_accent("c,"), "ç");
        // ストローク
        assert_eq!(convert_accent("O/"), "Ø");
        assert_eq!(convert_accent("o/"), "ø");
        // 上リング
        assert_eq!(convert_accent("A&"), "Å");
        assert_eq!(convert_accent("a&"), "å");
    }

    #[test]
    fn test_spec_ligatures() {
        // リガチャ（合字）
        assert_eq!(convert_accent("AE&"), "Æ");
        assert_eq!(convert_accent("ae&"), "æ");
        assert_eq!(convert_accent("OE&"), "Œ");
        assert_eq!(convert_accent("oe&"), "œ");
        // エスツェット
        assert_eq!(convert_accent("s&"), "ß");
    }

    #[test]
    fn test_spec_inverted() {
        // 逆転記号
        assert_eq!(convert_accent("!@"), "¡");
        assert_eq!(convert_accent("?@"), "¿");
    }

    #[test]
    fn test_spec_word_examples() {
        // 仕様書の例
        assert_eq!(convert_accent("cafe'"), "café");
        assert_eq!(convert_accent("pre'lude`"), "préludè");
        assert_eq!(convert_accent("nai:ve"), "naïve");
    }

    #[test]
    fn test_spec_invalid_accent() {
        // 無効なアクセント（そのまま出力）
        assert_eq!(convert_accent("z'"), "z'"); // 未定義の組み合わせ
        assert_eq!(convert_accent("ABC"), "ABC"); // アクセント記号なし
    }

    #[test]
    fn test_parse_accent_jis_code() {
        // parse_accent関数がJISコードを正しく返すか
        let result = parse_accent("A'");
        assert_eq!(result.len(), 1);
        match &result[0] {
            AccentPart::Accent {
                jis_code, unicode, ..
            } => {
                assert_eq!(jis_code, "1-09-24");
                assert_eq!(unicode, "Á");
            }
            _ => panic!("Expected AccentPart::Accent"),
        }
    }

    #[test]
    fn test_parse_accent_mixed() {
        // テキストとアクセントが混在
        let result = parse_accent("cafe'");
        assert_eq!(result.len(), 2);
        match &result[0] {
            AccentPart::Text(s) => assert_eq!(s, "caf"),
            _ => panic!("Expected Text"),
        }
        match &result[1] {
            AccentPart::Accent { unicode, .. } => assert_eq!(unicode, "é"),
            _ => panic!("Expected Accent"),
        }
    }
}
