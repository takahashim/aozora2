//! アクセント分解記法の変換

use once_cell::sync::Lazy;
use std::collections::HashMap;

use crate::delimiters::ACCENT_MARKS;
use crate::jis_table::jis_to_unicode;

/// アクセントテーブル（基底文字+記号 → JISコード）。`data/accent_table.json` から生成。
static ACCENT_TABLE: Lazy<HashMap<&'static str, &'static str>> =
    Lazy::new(|| include!(concat!(env!("OUT_DIR"), "/accent_table.rs")));

/// アクセント文字の説明文（基底文字+記号 → 説明文）。`data/accent_table.json` の
/// 同じエントリから生成する。参照実装 aozora2html の accent_table.yml 由来で、
/// 規則から組み立てられない表記（ドイツ語エスツェット など）も含む。
static ACCENT_NAME_TABLE: Lazy<HashMap<&'static str, &'static str>> =
    Lazy::new(|| include!(concat!(env!("OUT_DIR"), "/accent_name_table.rs")));

/// アクセント分解記法を変換
///
/// `cafe'` → `café` のように、基底文字+アクセント記号を
/// アクセント付き文字に変換する。
///
/// クレート内に呼び出し元は無い（本文の変換は [`parse_accent`] を通る）。
/// 素の文字列変換だけが欲しい利用者向けの公開ユーティリティ。
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
    parse_accent(input)
        .iter()
        .map(|part| match part {
            AccentPart::Text(text) => text.as_str(),
            AccentPart::Accent { unicode, .. } => unicode.as_str(),
        })
        .collect()
}

/// 文字がアクセント記号かどうか
pub fn is_accent_mark(c: char) -> bool {
    ACCENT_MARKS.contains(&c)
}

/// 文字列にアクセント表に載っている組み合わせが含まれるか
///
/// 参照実装 aozora2html の AccentParser は、アクセント表に載っている
/// 「基底文字＋記号」の並びを見つけたときだけアクセントとして扱い、
/// ひとつも見つからなければ `〔` `〕` をそのまま出力する。
/// 記号文字が含まれるだけでは足りない（英文中のカンマなどで誤判定するため）。
pub fn contains_accent_sequence(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    (0..chars.len()).any(|i| accent_at(&chars, i).is_some())
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
        /// この文字が消費した元入力の char 数（3文字リガチャなら 3、他は 2）。
        ///
        /// 呼び出し側が元テキスト上の span を切り出すのに要る。ここで持たないと
        /// 変換結果から幅を逆引きする羽目になる。
        source_width: usize,
    },
}

/// アクセント分解記法をパースしてJISコード情報を含む結果を返す
///
/// レンダラーが画像出力を選べるよう、JISコード情報を保持する。
/// アクセント記法の**唯一の走査**で、[`convert_accent`] も
/// [`contains_accent_sequence`] もこれ（と [`accent_at`]）を通る。
pub fn parse_accent(input: &str) -> Vec<AccentPart> {
    let chars: Vec<char> = input.chars().collect();
    let mut result = Vec::new();
    let mut text_buffer = String::new();
    let mut i = 0;

    while i < chars.len() {
        if let Some(accent) = accent_at(&chars, i) {
            let AccentPart::Accent { source_width, .. } = accent else {
                unreachable!("accent_at は Accent だけを返す")
            };
            // バッファのテキストを先に出力
            if !text_buffer.is_empty() {
                result.push(AccentPart::Text(std::mem::take(&mut text_buffer)));
            }
            i += source_width;
            result.push(accent);
            continue;
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

/// `chars[i]` から始まるアクセント文字（無ければ None）。
///
/// **アクセント記法の文法はここにだけ置く**: 3文字のリガチャ（`ae&` → æ）を
/// 2文字のアクセント（`e'` → é）より優先し、末尾がアクセント記号で、かつ
/// アクセント表に載っていて Unicode に解決できる組み合わせだけを認める。
fn accent_at(chars: &[char], i: usize) -> Option<AccentPart> {
    for source_width in [3, 2] {
        let end = i + source_width;
        if end > chars.len() || !is_accent_mark(chars[end - 1]) {
            continue;
        }
        let key: String = chars[i..end].iter().collect();
        if let Some((jis_code, unicode)) = lookup_accent(&key) {
            return Some(AccentPart::Accent {
                jis_code,
                name: accent_name(&key),
                unicode,
                source_width,
            });
        }
    }
    None
}

/// アクセントテーブルを検索してJISコードとUnicode文字の両方を返す
fn lookup_accent(key: &str) -> Option<(String, String)> {
    ACCENT_TABLE.get(key).and_then(|jis_code| {
        jis_to_unicode(jis_code).map(|unicode| (jis_code.to_string(), unicode))
    })
}

/// アクセント文字の説明文を引く。
///
/// 説明文は `data/accent_table.json` の各エントリが JIS コードと組で持っている。
/// 規則から組み立てられない表記（ドイツ語エスツェット、参照実装の表記ゆれ）が
/// あるため生成はせず表を引くだけにする。キーに直す fallback は保険。
fn accent_name(key: &str) -> String {
    ACCENT_NAME_TABLE
        .get(key)
        .copied()
        .unwrap_or(key)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// アクセント表にある組み合わせがなければアクセント記法とみなさない。
    /// 英文中のカンマなどを記号と誤認しないため。
    #[test]
    fn test_contains_accent_sequence() {
        assert!(contains_accent_sequence("E'difice"));
        assert!(contains_accent_sequence("ae&"));
        assert!(!contains_accent_sequence("参考"));
        assert!(!contains_accent_sequence(
            "欄外 Emil Brunner, Erlebnis, Erkenntnis und Glaube, 1923."
        ));
    }

    /// 説明文の表がアクセント表を完全に覆っていること。
    ///
    /// 元は 2 つの JSON に分かれていて 1:1 が偶然に頼っていたので、規則から説明文を
    /// 組み立てる（到達しない）分岐を持っていた。現在は 1 エントリが
    /// `{ jis_code, name }` の組を持ち build.rs が両マップを生成するので構造的に
    /// 揃うが、生成の取りこぼしを検出する網として残す。
    #[test]
    fn accent_name_table_covers_every_accent_key() {
        for key in ACCENT_TABLE.keys() {
            assert!(
                ACCENT_NAME_TABLE.contains_key(key),
                "説明文の無いアクセントキー: {key}"
            );
        }
    }

    /// 消費した元入力の幅を [`AccentPart::Accent`] 自身が持つ。以前は呼び出し側が
    /// convert_accent を掛け直して文字列比較で逆引きしていた。
    #[test]
    fn accent_parts_carry_their_source_width() {
        let parts = parse_accent("ae&e'x");
        assert_eq!(parts.len(), 3);
        assert!(matches!(
            parts[0],
            AccentPart::Accent {
                source_width: 3,
                ..
            }
        ));
        assert!(matches!(
            parts[1],
            AccentPart::Accent {
                source_width: 2,
                ..
            }
        ));
        assert_eq!(parts[2], AccentPart::Text("x".to_string()));
    }

    /// 3種の入口が同じ走査を通ること（convert_accent と contains_accent_sequence は
    /// parse_accent の派生）。
    #[test]
    fn entry_points_agree_with_parse_accent() {
        for input in ["cafe'", "ae&", "hello", "z'", "!@", "参考", "pre'lude`"] {
            let parts = parse_accent(input);
            let joined: String = parts
                .iter()
                .map(|p| match p {
                    AccentPart::Text(t) => t.as_str(),
                    AccentPart::Accent { unicode, .. } => unicode.as_str(),
                })
                .collect();
            assert_eq!(convert_accent(input), joined, "convert_accent: {input}");
            assert_eq!(
                contains_accent_sequence(input),
                parts.iter().any(|p| matches!(p, AccentPart::Accent { .. })),
                "contains_accent_sequence: {input}"
            );
        }
    }

    /// 説明文は参照実装 aozora2html の accent_table.yml をそのまま使う。
    /// 規則から組み立てられない表記が混じっているため。
    #[test]
    fn test_accent_names_come_from_the_reference_table() {
        assert_eq!(accent_name("AE&"), "リガチャAE");
        assert_eq!(accent_name("OE&"), "リガチャOE大文字");
        assert_eq!(accent_name("ae&"), "リガチャAE小文字");
        assert_eq!(accent_name("oe&"), "リガチャOE小文字");
        // エスツェット。規則どおりなら「アクセント付きS小文字」になってしまう
        assert_eq!(accent_name("s&"), "ドイツ語エスツェット");
        // 参照実装の表記ゆれ（A^ だけ字母 A が落ちている）もデータどおり再現する。
        // 訂正は quirk accent_name_typos オフ時にレンダラ側で行う。
        assert_eq!(accent_name("A^"), "サーカムフレックスアクセント付き");
        assert_eq!(accent_name("!@"), "逆感嘆符");
    }

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
