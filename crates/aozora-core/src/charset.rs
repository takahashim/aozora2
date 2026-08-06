//! 文字がどの文字集合に属するかの判定。
//!
//! 青空文庫形式は JIS X 0208 を前提に書き、その外の文字は外字注記 `※［＃…］` で
//! 書くのが規則。とはいえ「外」にも段階があり、書き手にとっての意味が違う。
//!
//! | 区分 | 意味 | 字数 |
//! |------|------|------|
//! | [`CharClass::Ascii`] | 英数記号 | 128 |
//! | [`CharClass::X0208`] | JIS X 0208。そのまま書いてよい | 6,879 |
//! | [`CharClass::X0201`] | 半角カナ・`¥`・`‾`。全角に直すべきもの | 65 |
//! | [`CharClass::X0213`] | JIS X 0213 の第3・第4水準。外字注記で面区点を書ける | 4,354 |
//! | [`CharClass::Other`] | JIS のどれにも無い。説明的な外字注記が要る | 残り |
//!
//! `6,879 + 4,354 = 11,233` は JIS X 0213 の収録字数と一致する（[`crate::jis_table`] の
//! 表がそのまま根拠になっている）。ただしこれは**枠（面区点）の数**で、Unicode の
//! 符号位置を数えると少し多くなる。同じ枠に綴りが 2 通りある文字（`¢` は U+00A2 と
//! U+FFE0）があるためで、判定はどちらの綴りでも同じ区分を返す。
//!
//! 判定は [`char_class`]。エディタのように打鍵のたびに全文を走る用途向けに、
//! BMP 全体を畳んだ表を [`mark_table`] が返す（フロントはこれを引くだけで済む）。

use crate::encoding::{is_directly_writable, normalize_char_for_shift_jis};
use crate::jis_table::unicode_values;
use encoding_rs::SHIFT_JIS;
use once_cell::sync::Lazy;
use std::collections::HashSet;

/// 文字が属する文字集合。
///
/// 判定の順序は Ascii → X0201 → X0208 → X0213 → Other。X 0208 の文字は
/// X 0213 の表にも載っているので、先に X 0208 を見る。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharClass {
    /// ASCII（U+0000〜U+007F）
    Ascii,
    /// JIS X 0201 の片仮名・記号。Shift_JIS では 1 バイトになる
    /// （半角カナ 63 字＋`¥` U+00A5＋`‾` U+203E）。
    X0201,
    /// JIS X 0208。青空文庫形式の本文に直接書ける。
    X0208,
    /// JIS X 0213 の追加分（第3水準＝面1、第4水準＝面2）。
    ///
    /// 外字注記に面区点を書ける（`※［＃「…」、第3水準1-15-23］`）。
    X0213,
    /// JIS のどの規格にも無い（絵文字・CJK 拡張の大半など）。
    Other,
}

/// 文字が属する文字集合を返す。
///
/// # Examples
///
/// ```
/// use aozora_core::charset::{char_class, CharClass};
///
/// assert_eq!(char_class('あ'), CharClass::X0208);
/// assert_eq!(char_class('ｱ'), CharClass::X0201);
/// assert_eq!(char_class('①'), CharClass::X0213); // 第3水準 1-13-01
/// assert_eq!(char_class('😀'), CharClass::Other);
/// ```
///
/// 2 コードポイントで 1 文字になる組（`か゚` など 25 組）は 1 文字ずつでは判定できない。
/// [`composed_x0213`] を参照のこと。
pub fn char_class(c: char) -> CharClass {
    if c.is_ascii() {
        return CharClass::Ascii;
    }
    // 同じ JIS の枠を指す符号位置の揺れ（macOS の日本語入力が作る `〜` U+301C など）は
    // 先に寄せる。保存時にどうせ寄せるので入力としては問題なく、診断
    // （`analysis` の non-x0208-char）も同じ扱いにしている。
    let c = normalize_char_for_shift_jis(c);
    if is_x0201(c) {
        return CharClass::X0201;
    }
    if is_directly_writable(c) {
        return CharClass::X0208;
    }
    if X0213_SINGLE.contains(&c) {
        return CharClass::X0213;
    }
    CharClass::Other
}

/// JIS X 0201 の文字か（＝Shift_JIS で 1 バイトになる非 ASCII の図形文字）。
///
/// WHATWG の shift_jis は半角カナ（U+FF61〜U+FF9F の 63 字）に加えて `¥` U+00A5 → `0x5C`、
/// `‾` U+203E → `0x7E` を 1 バイトにする。合わせて 65 字。
///
/// U+0080 も 1 バイト（`0x80`）になるが、これは制御文字で JIS X 0201 の文字ではないので
/// 除く。
fn is_x0201(c: char) -> bool {
    if c.is_control() {
        return false;
    }
    let mut buf = [0u8; 4];
    let (bytes, _, had_errors) = SHIFT_JIS.encode(c.encode_utf8(&mut buf));
    !had_errors && bytes.len() == 1
}

/// JIS X 0213 に載る文字のうち、1 コードポイントで表せるもの。
///
/// X 0208 の分も含む（[`char_class`] は先に X 0208 を判定するので重ならない）。
static X0213_SINGLE: Lazy<HashSet<char>> = Lazy::new(|| {
    unicode_values()
        .filter_map(|s| {
            let mut it = s.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        })
        .collect()
});

/// JIS X 0213 の文字のうち、2 コードポイントの合成で表すもの（25 組）。
///
/// 仮名＋結合半濁点（`か゚` = U+304B U+309A = 1-4-87 など）。1 文字ずつ見ると
/// 「か＝X 0208 ／ ゚＝Other」とちぐはぐになるので、組で扱う必要がある。
/// 並びは決定的（辞書順）。
pub fn composed_x0213() -> &'static [&'static str] {
    static COMPOSED: Lazy<Vec<&'static str>> = Lazy::new(|| {
        let mut v: Vec<&'static str> = unicode_values().filter(|s| s.chars().count() > 1).collect();
        v.sort_unstable();
        v.dedup();
        v
    });
    &COMPOSED
}

// --- エディタ向けの畳んだ表 -------------------------------------------------------

/// 表の値: 色を付けない（ASCII または JIS X 0208）。
pub const MARK_PLAIN: u8 = 1;
/// 表の値: JIS X 0201。
pub const MARK_X0201: u8 = 2;
/// 表の値: JIS X 0213 の第3・第4水準。
pub const MARK_X0213: u8 = 3;
/// 表の値: JIS のどれにも無い。
///
/// BMP の大半がこれなので、表のバイト列が 0 で埋まるよう 0 に割り当てている
/// （IPC で運ぶときの大きさが変わる）。
pub const MARK_OTHER: u8 = 0;

/// エディタが文字種を色分けするための表。
///
/// 起動時に一度だけ渡し、以降フロントはこれを引くだけで判定できる（IPC も解析も
/// 不要になる）。ASCII と X 0208 はどちらも色を付けないので [`MARK_PLAIN`] に畳む。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CharsetTable {
    /// BMP（U+0000〜U+FFFF）の 1 文字 2 ビット、下位ビットから詰めた 16,384 バイト。
    /// `bmp[cp >> 2] >> ((cp & 3) * 2) & 3` が [`MARK_PLAIN`] 等の値。
    pub bmp: Vec<u8>,
    /// BMP の外にある X 0213 の文字（第4水準の CJK 拡張B など 303 字）。
    /// ここに無い BMP 外の文字は [`MARK_OTHER`]。
    pub astral: Vec<u32>,
    /// 2 コードポイントで 1 文字になる組（[`composed_x0213`] と同じ 25 組）。
    pub composed: Vec<String>,
}

/// [`CharsetTable`] を組み立てる。
pub fn mark_table() -> CharsetTable {
    let mut bmp = vec![0u8; 0x10000 / 4];
    for cp in 0..0x10000u32 {
        let mark = match char::from_u32(cp) {
            // サロゲート符号位置は文字にならないので Other 扱い（表は引かれない）。
            None => MARK_OTHER,
            Some(c) => mark_of(char_class(c)),
        };
        if mark != MARK_OTHER {
            bmp[(cp >> 2) as usize] |= mark << ((cp & 3) * 2);
        }
    }

    let astral = unicode_values()
        .filter_map(|s| {
            let mut it = s.chars();
            match (it.next(), it.next()) {
                (Some(c), None) if c as u32 > 0xFFFF => Some(c as u32),
                _ => None,
            }
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    let composed = composed_x0213().iter().map(|s| s.to_string()).collect();

    CharsetTable {
        bmp,
        astral,
        composed,
    }
}

/// 区分を表の値に畳む。
fn mark_of(class: CharClass) -> u8 {
    match class {
        CharClass::Ascii | CharClass::X0208 => MARK_PLAIN,
        CharClass::X0201 => MARK_X0201,
        CharClass::X0213 => MARK_X0213,
        CharClass::Other => MARK_OTHER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BMP を総当たりして区分ごとの字数を数える。
    fn bmp_counts() -> Vec<(CharClass, usize)> {
        let mut n = [0usize; 5];
        for cp in 0..0x10000u32 {
            let Some(c) = char::from_u32(cp) else {
                continue;
            };
            let i = match char_class(c) {
                CharClass::Ascii => 0,
                CharClass::X0201 => 1,
                CharClass::X0208 => 2,
                CharClass::X0213 => 3,
                CharClass::Other => 4,
            };
            n[i] += 1;
        }
        vec![
            (CharClass::Ascii, n[0]),
            (CharClass::X0201, n[1]),
            (CharClass::X0208, n[2]),
            (CharClass::X0213, n[3]),
            (CharClass::Other, n[4]),
        ]
    }

    /// ASCII と JIS X 0201 の字数。どちらも Unicode 側の綴りが 1 通りなので、
    /// 符号位置を数えてそのまま規格の字数と突き合わせられる。
    #[test]
    fn counts_match_the_standards() {
        let counts = bmp_counts();
        let get = |k: CharClass| counts.iter().find(|(c, _)| *c == k).unwrap().1;

        assert_eq!(get(CharClass::Ascii), 128);
        // 半角カナ 63 字 ＋ ¥ ＋ ‾
        assert_eq!(get(CharClass::X0201), 65);
    }

    /// JIS X 0213 の全 11,233 枠が「直接書ける」側と「外字注記で書く」側に過不足なく割れ、
    /// **JIS の文字が [`CharClass::Other`] に落ちることは無い**。
    ///
    /// **符号位置ではなく枠（面区点）を数える**。同じ枠を指す Unicode の綴りが複数ある
    /// （`¢` は U+00A2 と U+FFE0）ため、符号位置を数えても規格の字数にはならない。
    #[test]
    fn x0213_cells_split_into_writable_and_gaiji() {
        let (mut writable, mut additions) = (0usize, 0usize);
        for value in unicode_values() {
            match value.chars().count() {
                // 2 コードポイントの合成（か゚ など）は必ず外字注記側。
                2 => additions += 1,
                1 => match char_class(value.chars().next().unwrap()) {
                    // 半角の綴りが当たっている枠が 4 つある（1-01-17 `‾`、1-01-79 `¥`、
                    // 1-01-32 `\`、1-02-18 `~`）。文字としては半角と判定するのが正しい
                    // ——Shift_JIS へ書くと 1 バイトになるので、エディタでは全角に
                    // 直すよう促したい。枠としてはここでは書ける側に数える。
                    CharClass::X0208 | CharClass::X0201 | CharClass::Ascii => writable += 1,
                    CharClass::X0213 => additions += 1,
                    other => panic!("{value} が JIS の外と判定された: {other:?}"),
                },
                n => panic!("{value} は {n} コードポイント（想定は 1 か 2）"),
            }
        }

        // 規格上の JIS X 0208 は 6,879 枠（非漢字 524 ＋ 第1・第2水準漢字 6,355）。
        // ここで 3 つ多いのは、X 0213 が別枠として持つ次の 3 枠の綴りが、Shift_JIS では
        // X 0208 のバイトになるため。保存しても失われないので「直接書ける」側に入れる。
        //
        //   1-02-17 `－` U+FF0D → 1-01-61 のバイト
        //   1-02-18 `~`  U+007E → ASCII のまま
        //   1-02-52 `∥` U+2225 → 1-01-34 のバイト
        assert_eq!(writable, 6879 + 3, "直接書ける枠の数が変わった");
        assert_eq!(additions, 4354 - 3, "外字注記で書く枠の数が変わった");
        assert_eq!(
            writable + additions,
            11233,
            "JIS X 0213 の収録字数と合わない"
        );
    }

    /// 追加分のうち BMP の外と合成のものは表に別枠で載せる（ビットマップに入らない）。
    #[test]
    fn table_carries_what_the_bitmap_cannot_hold() {
        let table = mark_table();
        assert_eq!(table.astral.len(), 303, "BMP 外の JIS 文字");
        assert_eq!(table.composed.len(), 25, "2 コードポイントの合成");
    }

    /// 同じ枠を指す綴り違いは、どちらの綴りでも X 0208 と判定する。
    ///
    /// macOS の日本語入力は `〜` を U+301C、`—` を U+2014 で入れる。保存時に寄せる
    /// （[`normalize_char_for_shift_jis`]）ので入力として問題はなく、色を分けると
    /// 誤解を招く。診断 `non-x0208-char` も同じ扱いにしている。
    #[test]
    fn alternate_spellings_are_treated_as_x0208() {
        for c in [
            '\u{301C}', '\u{2014}', '\u{2016}', '\u{00A2}', '\u{00A3}', '\u{00AC}',
        ] {
            assert_eq!(char_class(c), CharClass::X0208, "U+{:04X}", c as u32);
            // 寄せた先も当然 X 0208。
            assert_eq!(
                char_class(normalize_char_for_shift_jis(c)),
                CharClass::X0208
            );
        }
    }

    #[test]
    fn classifies_representative_characters() {
        // X 0208。普通の本文はすべてこれ。
        assert_eq!(char_class('あ'), CharClass::X0208);
        assert_eq!(char_class('漢'), CharClass::X0208);
        assert_eq!(char_class('熙'), CharClass::X0208); // 第2水準の末尾 84-06
                                                        // X 0201。全角に直すべきもの。
        assert_eq!(char_class('ｱ'), CharClass::X0201);
        assert_eq!(char_class('ﾞ'), CharClass::X0201);
        assert_eq!(char_class('¥'), CharClass::X0201);
        assert_eq!(char_class('‾'), CharClass::X0201);
        // 第3水準（面1）。CP932 が符号化できるものもここに入る。
        assert_eq!(char_class('①'), CharClass::X0213); // 1-13-01
        assert_eq!(char_class('㈱'), CharClass::X0213);
        assert_eq!(char_class('ヷ'), CharClass::X0213); // 1-07-82 濁点付き片仮名
                                                        // 第4水準（面2）。BMP の外にもある。
        assert_eq!(char_class('𠮟'), CharClass::X0213);
        // JIS に無い。
        assert_eq!(char_class('😀'), CharClass::Other);
        assert_eq!(char_class('仼'), CharClass::Other); // CP932 では書けるが X 0213 に無い
    }

    /// 表の引き方（フロントと同じ手順）が [`char_class`] と一致する。
    #[test]
    fn table_agrees_with_char_class() {
        let table = mark_table();
        for cp in 0..0x10000u32 {
            let Some(c) = char::from_u32(cp) else {
                continue;
            };
            let got = table.bmp[(cp >> 2) as usize] >> ((cp & 3) * 2) & 3;
            assert_eq!(got, mark_of(char_class(c)), "U+{cp:04X} ({c}) がずれている");
        }
    }

    /// BMP 外の一覧は第3・第4水準だけで、重複も取りこぼしもない。
    #[test]
    fn astral_list_is_sorted_and_unique() {
        let table = mark_table();
        assert!(table.astral.windows(2).all(|w| w[0] < w[1]));
        assert!(table.astral.iter().all(|&cp| cp > 0xFFFF));
        assert!(table.astral.contains(&('𠮟' as u32)));
    }

    /// 合成の組は仮名＋結合半濁点で、片方だけでは X 0213 と判定されない。
    #[test]
    fn composed_pairs_need_both_code_points() {
        let composed = composed_x0213();
        assert!(composed.contains(&"か゚"));
        assert!(composed.iter().all(|s| s.chars().count() == 2));
        // 単独では別の区分になる（だから組で見る必要がある）。
        assert_eq!(char_class('か'), CharClass::X0208);
        assert_eq!(char_class('\u{309A}'), CharClass::Other);
    }
}
