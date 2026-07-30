//! エンコーディング判定・変換

use encoding_rs::SHIFT_JIS;
use once_cell::sync::Lazy;
use std::collections::HashSet;

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

/// Shift_JIS で書き出すときに、どこまでの文字を許すか。
///
/// `crate::html::Quirks` と同じ考え方で、**既定は既存資産に合わせ**、規格に沿った
/// 厳しい側を選べるようにする。既定を厳しくすると、読み込んで一文字も触らずに
/// 保存し直すだけで失敗する文書が出てしまい、エディタとして往復できない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CharsetPolicy {
    /// **既定**。Shift_JIS（WHATWG/CP932）で符号化できれば通す。
    ///
    /// 半角カナや NEC 特殊文字・IBM 拡張を本文に直接含む文書が実在し
    /// （17,899 中 25 件）、参照実装はそれらを**バイトのまま通す**ので今日も
    /// byte 一致で変換できている（例: 001149/43551 の `Id. c. ⅹ. p. 348`）。
    /// 開いて保存し直すだけで壊れないよう、既定はこちら。
    /// 「直接書くべきでない文字」は保存を止めるのではなく診断で知らせる
    /// （`crate::analysis` の `non-x0208-char`）。
    #[default]
    Cp932,
    /// 青空文庫形式の入力規則に沿う。ASCII と JIS X 0208 だけを許す
    /// （[`is_directly_writable`]）。X 0208 の外は外字注記 `※［＃…］` で書く。
    X0208,
}

impl CharsetPolicy {
    /// この方針でその文字を直接書けるか。
    fn allows(self, c: char) -> bool {
        match self {
            CharsetPolicy::Cp932 => is_shift_jis_encodable(c),
            CharsetPolicy::X0208 => is_directly_writable(c),
        }
    }
}

/// Shift_JIS（WHATWG/CP932）で符号化できるか。X 0208 の外でも、半角カナや
/// NEC/IBM 拡張のように符号位置があるものは通る。
pub fn is_shift_jis_encodable(c: char) -> bool {
    if c.is_ascii() {
        return true;
    }
    let mut buf = [0u8; 4];
    let (_, _, had_errors) = SHIFT_JIS.encode(c.encode_utf8(&mut buf));
    !had_errors
}

/// 本文に直接書けない（外字注記で書くべき）文字と、その位置（0 起点の行・char 桁）。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct UnencodableChar {
    /// 本文に直接書けない文字
    pub ch: char,
    /// 行番号（0 起点）
    pub line: usize,
    /// 行内の char 位置（0 起点）
    pub column: usize,
}

/// 青空文庫形式の本文に**直接書ける**文字か。許すのは ASCII と JIS X 0208 だけ。
///
/// JIS X 0208 の外の文字は外字注記 `※［＃…］` で書くのが青空文庫形式のルールなので、
/// 直接書かれていれば入力の誤りとして扱う。除かれるのは次のもの。
///
/// - **JIS X 0201**（半角カナ、および 1 バイトに割り当てられる `¥` U+00A5 と
///   `‾` U+203E）。バイト列としては Shift_JIS で表せるが X 0208 ではない。
/// - **CP932 の拡張**（NEC 特殊文字 区13 の ①Ⅰ㈱㎜、NEC 選定 IBM 拡張 区89-92、
///   IBM 拡張 区115-119 の 﨑 など）。WHATWG/CP932 は符号化できるが X 0208 ではなく、
///   参照実装 aozora2html が使う Ruby の `Encoding::Shift_JIS` も符号化できない。
/// - **JIS X 0213 の追加分**（第3・第4水準の漢字、濁点付き片仮名 ヷヸヹヺ など）。
///
/// X 0208 は面 1 の区 1〜8（記号・英数・かな・ギリシャ・キリル・罫線）＋
/// 区 16〜84（第1・第2水準漢字）で、CP932 の拡張はいずれもこの区の外に置かれて
/// いるため、区の範囲だけで切り分けられる。
///
/// 判定は1文字ずつ符号化するより表を引く方が速い。診断（`crate::analysis`）が
/// 打鍵のたびに全文を走査するので、実測で 18.6ms/36万字が効いてしまう。
/// 表は [`X0208_CHARS`] が起動時に1度だけ組み立てる（符号化による判定と
/// 一致することは `test_table_matches_encoder` が BMP 全体で固定する）。
pub fn is_directly_writable(c: char) -> bool {
    c.is_ascii() || X0208_CHARS.contains(&c)
}

/// JIS X 0208（面 1 の区 1〜8・16〜84）に含まれる文字の集合。
///
/// 「符号化して区を見る」判定（[`is_x0208_by_encoder`]）を BMP 全体に一度だけ
/// 適用して作る。区点から復号して組み立てると、**符号化器だけが受ける別符号位置**
/// （U+2212 は復号側には現れないが 1 区 61 点へ符号化できる）を取りこぼすため、
/// 判定そのものから導く。組み立ては初回参照時の一度きり。
static X0208_CHARS: Lazy<HashSet<char>> = Lazy::new(|| {
    (0u32..=0xFFFF)
        .filter_map(char::from_u32)
        .filter(|c| !c.is_ascii() && is_x0208_by_encoder(*c))
        .collect()
});

/// 符号化して区を見る X 0208 判定（[`X0208_CHARS`] の作成元＝定義そのもの）。
///
/// BMP の外（サロゲートペアで表す第3・第4水準など）は Shift_JIS に符号位置が
/// 無いので、表に載らず [`is_directly_writable`] は false になる。
fn is_x0208_by_encoder(c: char) -> bool {
    let mut buf = [0u8; 4];
    let (bytes, _, had_errors) = SHIFT_JIS.encode(c.encode_utf8(&mut buf));
    if had_errors || bytes.len() != 2 {
        // 2 バイトにならないものは X 0201（半角カナ・¥・‾）か符号化不能。
        return false;
    }
    matches!(shift_jis_ku(bytes[0], bytes[1]), 1..=8 | 16..=84)
}

/// Shift_JIS の 2 バイトから区（1〜119）を求める。
fn shift_jis_ku(lead: u8, trail: u8) -> u32 {
    let lead_offset = if lead <= 0x9F { 0x81 } else { 0xC1 };
    let ku = (lead - lead_offset) as u32 * 2 + 1;
    if trail >= 0x9F {
        ku + 1
    } else {
        ku
    }
}

/// Shift_JIS で符号化できない符号位置を、同じ文字を表す符号化可能な符号位置に寄せる。
///
/// 対応表は [`SHIFT_JIS_EQUIVALENTS`]。**同一の文字**の符号位置違いだけを直すので、
/// 別の文字（絵文字や JIS にない漢字など）はそのまま残り、[`encode_shift_jis`] が
/// エラーとして報告する。
pub fn normalize_for_shift_jis(text: &str) -> String {
    text.chars().map(normalize_char_for_shift_jis).collect()
}

/// [`normalize_for_shift_jis`] の1文字版。
///
/// 保存時にどうせ寄せる符号位置を、診断（`crate::analysis` の `non-x0208-char`）が
/// 誤って警告しないために公開している。macOS の日本語入力が作る U+301C や U+2014 は
/// そのままでは符号化できないが、書き出す前に寄せるので入力としては問題ない。
pub fn normalize_char_for_shift_jis(c: char) -> char {
    SHIFT_JIS_EQUIVALENTS
        .iter()
        .find(|(from, _)| *from == c)
        .map_or(c, |(_, to)| *to)
}

/// Shift_JIS に符号化する。`policy` で許されない文字が 1 つでもあればエラーにする。
///
/// 既定の [`CharsetPolicy::Cp932`] は符号化できれば通す（既存ファイルが往復できる）。
/// [`CharsetPolicy::X0208`] を選ぶと青空文庫形式の入力規則どおり ASCII と
/// JIS X 0208 だけに絞り、①や﨑のような CP932 拡張も拒否する。
///
/// encoding_rs の `encode` は WHATWG 仕様どおり、符号化できない文字を数値文字参照
/// （`&#12316;` 等）の**文字列**に置き換える。黙って置き換えず、直すべき箇所として
/// 位置つきで返すため、符号化の前に全文を検査する。
pub fn encode_shift_jis(
    text: &str,
    policy: CharsetPolicy,
) -> Result<Vec<u8>, Vec<UnencodableChar>> {
    let rejected = chars_not_allowed(text, policy);
    if !rejected.is_empty() {
        return Err(rejected);
    }
    let (bytes, _, had_errors) = SHIFT_JIS.encode(text);
    debug_assert!(
        !had_errors,
        "検査を通った文字列は符号化できるはず（X 0208 判定と符号化器の食い違い）"
    );
    Ok(bytes.into_owned())
}

/// 方針で許されない文字を位置つきで拾う。
fn chars_not_allowed(text: &str, policy: CharsetPolicy) -> Vec<UnencodableChar> {
    let mut found = Vec::new();
    for (line, line_text) in text.split('\n').enumerate() {
        for (column, ch) in line_text.chars().enumerate() {
            if !policy.allows(ch) {
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
        let bytes = encode_shift_jis(text, CharsetPolicy::default()).expect("符号化できる");
        assert_eq!(decode_to_utf8(&bytes), text);
    }

    /// 対応表の左側（JIS 本来の符号位置）はそのままでは符号化できず、
    /// 正規化すると符号化できる。表の各行が実際にその性質を持つことを縛る。
    #[test]
    fn test_normalize_makes_equivalents_encodable() {
        for (from, to) in SHIFT_JIS_EQUIVALENTS {
            let from_text = from.to_string();
            assert!(
                encode_shift_jis(&from_text, CharsetPolicy::default()).is_err(),
                "{from:?} は素では符号化できないはず（表の前提が崩れている）"
            );
            assert_eq!(normalize_for_shift_jis(&from_text), to.to_string());
            assert!(
                encode_shift_jis(&to.to_string(), CharsetPolicy::default()).is_ok(),
                "{to:?} は符号化できるはず"
            );
        }
    }

    /// 波ダッシュを含む文は、正規化してから符号化すると Shift_JIS の 0x8160 になる。
    #[test]
    fn test_normalize_wave_dash() {
        let bytes = encode_shift_jis(&normalize_for_shift_jis("あ〜"), CharsetPolicy::default())
            .expect("符号化できる");
        assert_eq!(bytes, vec![0x82, 0xA0, 0x81, 0x60]);
    }

    /// 符号化できない文字は、数値文字参照に置き換えず位置つきで返す。
    #[test]
    fn test_encode_shift_jis_reports_unencodable() {
        let err = encode_shift_jis("１行目\n２行目に😀と🎌", CharsetPolicy::default())
            .expect_err("符号化できない");
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

    /// ASCII と JIS X 0208 は直接書ける。
    #[test]
    fn test_directly_writable_accepts_ascii_and_x0208() {
        for c in [
            'a', '1', '<', '&', 'あ', 'ア', '亜', '熙', '々', '※', '●', '≒', '　', 'Α', 'а',
        ] {
            assert!(
                is_directly_writable(c),
                "{c:?} は直接書ける（ASCII か X 0208）"
            );
        }
    }

    /// CP932 の拡張（NEC 特殊文字・IBM 拡張）は符号化できてしまうが、X 0208 では
    /// ないので拒否する。参照実装の Ruby `Encoding::Shift_JIS` と同じ線引き。
    #[test]
    fn test_directly_writable_rejects_cp932_extensions() {
        for c in ['①', 'Ⅰ', '㈱', '㎜', '﨑'] {
            let mut buf = [0u8; 4];
            let (bytes, _, had_errors) = SHIFT_JIS.encode(c.encode_utf8(&mut buf));
            assert!(
                !had_errors,
                "{c:?} は CP932 では符号化できる（前提が崩れている）"
            );
            assert_eq!(bytes.len(), 2);
            assert!(!is_directly_writable(c), "{c:?} は X 0208 外なので拒否する");
        }
    }

    /// JIS X 0201（半角カナ・1 バイトの ¥ と ‾）も拒否する。
    #[test]
    fn test_directly_writable_rejects_x0201() {
        for c in ['ｱ', 'ﾞ', '｡', '\u{00A5}', '\u{203E}'] {
            assert!(!is_directly_writable(c), "{c:?} は X 0201 なので拒否する");
        }
    }

    /// X 0213 の追加分（第3・第4水準、濁点付き片仮名）も拒否する。
    #[test]
    fn test_directly_writable_rejects_x0213_additions() {
        for c in ['睜', '𥥔', 'ヷ'] {
            assert!(!is_directly_writable(c), "{c:?} は X 0208 外なので拒否する");
        }
    }

    /// 拒否した文字は符号化せず、位置つきで返す。
    #[test]
    fn test_encode_rejects_with_positions() {
        let err = encode_shift_jis("１行目\n２行目に①と睜", CharsetPolicy::X0208)
            .expect_err("拒否される");
        assert_eq!(
            err,
            vec![
                UnencodableChar {
                    ch: '①',
                    line: 1,
                    column: 4
                },
                UnencodableChar {
                    ch: '睜',
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
        assert!(
            encode_shift_jis(&normalize_for_shift_jis("あ😀"), CharsetPolicy::default()).is_err()
        );
    }

    /// 表引きが判定そのもの（符号化して区を見る）と BMP 全体で一致することを固定する。
    /// 表は初回参照時に一度だけ組み立てるので速いが、組み立て方を変えたときに
    /// 静かにずれないよう縛っておく。
    #[test]
    fn test_table_matches_encoder() {
        for cp in 0u32..=0xFFFF {
            let Some(c) = char::from_u32(cp) else {
                continue;
            };
            let expected = c.is_ascii() || is_x0208_by_encoder(c);
            assert_eq!(
                is_directly_writable(c),
                expected,
                "U+{cp:04X} {c:?} で表引きと判定がずれた"
            );
        }
    }

    /// 区点から復号して表を作ると取りこぼす符号位置（符号化器だけが受ける側）。
    /// U+2212（マイナス）は復号では現れないが 1 区 61 点へ符号化できるので
    /// X 0208 として扱う。表の作り方を変えたときの番人。
    #[test]
    fn test_encoder_only_codepoints_are_in_the_table() {
        assert!(
            is_directly_writable('\u{2212}'),
            "U+2212 は X 0208 として扱う"
        );
        assert!(is_directly_writable('\u{FF0D}'), "U+FF0D も同じ区点");
    }

    /// 既定（CP932）では、実在文書が含む半角カナ・NEC 特殊文字・IBM 拡張も保存できる。
    /// 開いて保存し直すだけで壊れないことがエディタとしての最低条件なので、
    /// ここを緩くしている（001149/43551 の `ⅹ` はオラクルで byte 一致している）。
    #[test]
    fn test_cp932_policy_allows_real_world_extensions() {
        for c in ['ⅹ', 'Ⅰ', '①', '﨑', '｣', '､', '･', 'ｱ'] {
            let text = c.to_string();
            assert!(
                encode_shift_jis(&text, CharsetPolicy::Cp932).is_ok(),
                "{c:?} は既定では保存できる"
            );
            assert!(
                encode_shift_jis(&text, CharsetPolicy::X0208).is_err(),
                "{c:?} は厳格側では拒否される"
            );
        }
    }

    /// どちらの方針でも、Shift_JIS に符号位置が無い文字は拒否する。
    #[test]
    fn test_both_policies_reject_unencodable() {
        for policy in [CharsetPolicy::Cp932, CharsetPolicy::X0208] {
            assert!(encode_shift_jis("あ😀", policy).is_err());
            assert!(encode_shift_jis("あ𥥔", policy).is_err());
        }
    }
}
