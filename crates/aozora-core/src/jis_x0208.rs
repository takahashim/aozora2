//! JIS X 0208 の文字集合の所属判定。
//!
//! いま必要なのは**漢字**（第1水準 2,965 字＋第2水準 3,390 字＝計 6,355 字）だけ。
//! ルビ親文字の文字種判定（[`crate::char_type`]）が「この字は漢字か」を問い合わせる。
//!
//! 表は `build.rs` が `data/jis2ucs.json`（JIS X 0213 の面区点 → Unicode）の
//! 面1 **16-01〜47-51** ＋ **48-01〜84-06** から生成する。X 0208 の漢字は Unicode 上では
//! 4,031 本のレンジに散らばるためレンジ列挙できないが、区点空間では上記 2 レンジで済む。
//! 生成したビットマップを引くので判定は O(1) で、エンコーディング実装にも依存しない。

// build.rs が生成した BASE / END / COUNT / BITS を取り込む。
// 基点・上限・ビット順の規約は生成ファイル側に書かれており、ここでは再定義しない
// （二重定義して食い違うのを防ぐため）。
include!(concat!(env!("OUT_DIR"), "/x0208_kanji_bitmap.rs"));

/// 文字が JIS X 0208 の漢字（第1水準＋第2水準）か。
///
/// 参照実装 `aozora2html` の `REGEX_KANJI = /[亜-熙…]/`（Shift_JIS 範囲）の
/// 亜-熙 に相当する。NEC/IBM 拡張漢字や JIS X 0213 の第3・第4水準は含まない。
pub(crate) fn is_kanji(c: char) -> bool {
    let cp = c as u32;
    if !(BASE..=END).contains(&cp) {
        return false;
    }
    let idx = (cp - BASE) as usize;
    BITS[idx / 8] & (1 << (idx % 8)) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧定義（検証用）: Shift_JIS へ符号化して 亜(0x889F)〜熙(0xEAA4) に入るか。
    /// ビットマップ化の前はこれが漢字判定だった。生成表がこれと完全一致することを
    /// 下のテストで保証する（一致する限り byte 一致は崩れない）。
    fn sjis_in_kanji_range(c: char) -> bool {
        let mut buf = [0u8; 8];
        let (encoded, _, had_err) = encoding_rs::SHIFT_JIS.encode(c.encode_utf8(&mut buf));
        if had_err {
            return false;
        }
        let b = encoded.as_ref();
        if b.len() != 2 {
            return false;
        }
        let code = ((b[0] as u16) << 8) | b[1] as u16;
        (0x889F..=0xEAA4).contains(&code)
    }

    #[test]
    fn bitmap_matches_sjis_definition() {
        // 対象範囲を総当たりし、生成したビットマップが旧定義と 1 文字も違わないことを確認する。
        // 基点やビット順を取り違えるとここで落ちる。
        let mut count = 0usize;
        for cp in BASE..=END {
            let c = char::from_u32(cp).expect("URO は全て有効なスカラ値");
            let expected = sjis_in_kanji_range(c);
            assert_eq!(
                is_kanji(c),
                expected,
                "U+{cp:04X} ({c}) の判定が旧定義と食い違う"
            );
            if expected {
                count += 1;
            }
        }
        assert_eq!(count, COUNT, "JIS X 0208 の漢字数が合わない");
    }

    #[test]
    fn bitmap_population_matches_count() {
        // 立っているビット数が収録字数と一致する（生成時の取りこぼし・重複の検出）。
        let population: u32 = BITS.iter().map(|b| b.count_ones()).sum();
        assert_eq!(population as usize, COUNT);
    }

    #[test]
    fn boundaries_and_extensions() {
        // 第1水準の先頭（16-01）と第2水準の末尾（84-06、X 0208-1990 で追加）。
        assert!(is_kanji('亜'));
        assert!(is_kanji('熙'));
        // NEC/IBM 拡張漢字は X 0208 の外（厓・賴）。
        assert!(!is_kanji('\u{5393}'));
        assert!(!is_kanji('\u{8CF4}'));
        // 々 などは URO の外なのでここでは false。漢字とみなすのは char_type 側の規則。
        assert!(!is_kanji('々'));
        // 対象範囲外は無条件で false。
        assert!(!is_kanji('あ'));
        assert!(!is_kanji('A'));
    }
}
