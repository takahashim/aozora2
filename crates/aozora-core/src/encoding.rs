//! エンコーディング判定・変換

use encoding_rs::SHIFT_JIS;

/// バイト列のエンコーディングを判定してUTF-8文字列に変換
///
/// # 判定ロジック
/// 1. UTF-8 BOMがあればUTF-8
/// 2. UTF-8として妥当ならUTF-8
/// 3. それ以外はShift_JIS
///
/// # Examples
///
/// ```
/// use aozora_core::encoding::decode_to_utf8;
///
/// let utf8_bytes = "こんにちは".as_bytes();
/// assert_eq!(decode_to_utf8(utf8_bytes), "こんにちは");
/// ```
pub fn decode_to_utf8(bytes: &[u8]) -> String {
    // BOMチェック
    let bytes = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..] // BOMをスキップ
    } else {
        bytes
    };

    // UTF-8として妥当かチェック
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }

    // Shift_JISとしてデコード
    let (cow, _, _) = SHIFT_JIS.decode(bytes);
    cow.into_owned()
}

/// Shift_JIS の同じ符号位置に対応する Unicode 符号位置の対（左＝JIS 本来の対応、
/// 右＝WHATWG/CP932 が Shift_JIS と対応づける側）。
///
/// 左側は encoding_rs（WHATWG）の Shift_JIS エンコーダで**符号化できない**。
/// macOS の日本語入力は「〜」を U+301C（波ダッシュ）で、「—」を U+2014（EM DASH）で
/// 入れるので、青空文庫のファイル（デコードすると右側になる）に手入力が混ざると
/// この形で現れる。値は encoding_rs 実測（tests の照合で固定）。
const SHIFT_JIS_EQUIVALENTS: &[(char, char)] = &[
    ('\u{301C}', '\u{FF5E}'), // 波ダッシュ → 全角チルダ
    ('\u{2014}', '\u{2015}'), // EM DASH → 水平線（ダッシュ）
    ('\u{2016}', '\u{2225}'), // 双柱 → 平行記号
    ('\u{00A2}', '\u{FFE0}'), // ¢ → 全角セント
    ('\u{00A3}', '\u{FFE1}'), // £ → 全角ポンド
    ('\u{00AC}', '\u{FFE2}'), // ¬ → 全角否定
];

/// Shift_JIS で符号化できなかった文字と、その位置（0 起点の行・char 桁）。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct UnencodableChar {
    /// 符号化できなかった文字
    pub ch: char,
    /// 行番号（0 起点）
    pub line: usize,
    /// 行内の char 位置（0 起点）
    pub column: usize,
}

/// Shift_JIS で符号化できない符号位置を、同じ文字を表す符号化可能な符号位置に寄せる。
///
/// 対応表は [`SHIFT_JIS_EQUIVALENTS`]。**同一の文字**の符号位置違いだけを直すので、
/// 別の文字（絵文字や JIS にない漢字など）はそのまま残り、[`encode_shift_jis`] が
/// エラーとして報告する。
pub fn normalize_for_shift_jis(text: &str) -> String {
    text.chars()
        .map(|c| {
            SHIFT_JIS_EQUIVALENTS
                .iter()
                .find(|(from, _)| *from == c)
                .map_or(c, |(_, to)| *to)
        })
        .collect()
}

/// Shift_JIS に符号化する。符号化できない文字が1つでもあればエラーにする。
///
/// encoding_rs の `encode` は WHATWG 仕様どおり、符号化できない文字を数値文字参照
/// （`&#12316;` 等）の**文字列**に置き換える。青空文庫形式では JIS にない文字は
/// 外字注記 `※［＃…］` で書く約束なので、黙って置き換えず、直すべき箇所として
/// 位置つきで返す。
pub fn encode_shift_jis(text: &str) -> Result<Vec<u8>, Vec<UnencodableChar>> {
    let (bytes, _, had_errors) = SHIFT_JIS.encode(text);
    if !had_errors {
        return Ok(bytes.into_owned());
    }
    Err(unencodable_chars(text))
}

/// 符号化できない文字を位置つきで拾う（Shift_JIS は文字単位で状態を持たないので、
/// 1文字ずつ試して判定できる）。
fn unencodable_chars(text: &str) -> Vec<UnencodableChar> {
    let mut found = Vec::new();
    for (line, line_text) in text.split('\n').enumerate() {
        for (column, ch) in line_text.chars().enumerate() {
            let (_, _, had_errors) = SHIFT_JIS.encode(ch.encode_utf8(&mut [0u8; 4]));
            if had_errors {
                found.push(UnencodableChar { ch, line, column });
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8() {
        let bytes = "こんにちは".as_bytes();
        assert_eq!(decode_to_utf8(bytes), "こんにちは");
    }

    #[test]
    fn test_utf8_with_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("こんにちは".as_bytes());
        assert_eq!(decode_to_utf8(&bytes), "こんにちは");
    }

    #[test]
    fn test_shift_jis() {
        // "こんにちは" in Shift_JIS
        let bytes = vec![0x82, 0xB1, 0x82, 0xF1, 0x82, 0xC9, 0x82, 0xBF, 0x82, 0xCD];
        assert_eq!(decode_to_utf8(&bytes), "こんにちは");
    }

    /// 普通の日本語は素通しで符号化でき、デコードして往復する。
    #[test]
    fn test_encode_shift_jis_roundtrip() {
        let text = "吾輩は猫である。名前はまだ無い。";
        let bytes = encode_shift_jis(text).expect("符号化できる");
        assert_eq!(decode_to_utf8(&bytes), text);
    }

    /// 対応表の左側（JIS 本来の符号位置）はそのままでは符号化できず、
    /// 正規化すると符号化できる。表の各行が実際にその性質を持つことを縛る。
    #[test]
    fn test_normalize_makes_equivalents_encodable() {
        for (from, to) in SHIFT_JIS_EQUIVALENTS {
            let from_text = from.to_string();
            assert!(
                encode_shift_jis(&from_text).is_err(),
                "{from:?} は素では符号化できないはず（表の前提が崩れている）"
            );
            assert_eq!(normalize_for_shift_jis(&from_text), to.to_string());
            assert!(
                encode_shift_jis(&to.to_string()).is_ok(),
                "{to:?} は符号化できるはず"
            );
        }
    }

    /// 波ダッシュを含む文は、正規化してから符号化すると Shift_JIS の 0x8160 になる。
    #[test]
    fn test_normalize_wave_dash() {
        let bytes = encode_shift_jis(&normalize_for_shift_jis("あ〜")).expect("符号化できる");
        assert_eq!(bytes, vec![0x82, 0xA0, 0x81, 0x60]);
    }

    /// 符号化できない文字は、数値文字参照に置き換えず位置つきで返す。
    #[test]
    fn test_encode_shift_jis_reports_unencodable() {
        let err = encode_shift_jis("１行目\n２行目に😀と🎌").expect_err("符号化できない");
        assert_eq!(
            err,
            vec![
                UnencodableChar {
                    ch: '😀',
                    line: 1,
                    column: 4
                },
                UnencodableChar {
                    ch: '🎌',
                    line: 1,
                    column: 6
                },
            ]
        );
    }

    /// 正規化は同じ文字の符号位置違いだけを直す。別の文字は残してエラーにする
    /// （外字注記で書くべき箇所を黙って潰さない）。
    #[test]
    fn test_normalize_keeps_other_characters() {
        assert_eq!(normalize_for_shift_jis("あ😀"), "あ😀");
        assert!(encode_shift_jis(&normalize_for_shift_jis("あ😀")).is_err());
    }
}
