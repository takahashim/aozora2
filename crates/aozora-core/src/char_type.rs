//! 文字種別判定
//!
//! ルビの親文字を自動抽出する際に使用する文字種別判定機能を提供します。
//!
//! # 文字種別一覧
//!
//! | 種別 | 説明 |
//! |------|------|
//! | Hiragana | ひらがな（ぁ-ん、ゝ、ゞ） |
//! | Katakana | カタカナ（ァ-ン、ー、ヽ、ヾ、ヴ） |
//! | Zenkaku | 全角英数・ギリシャ・キリル文字 |
//! | Hankaku | 半角英数と一部記号 |
//! | Kanji | JIS X 0208 の漢字 6,355 字 ＋ 々、※、仝、〆、〇、ヶ |
//! | HankakuTerminate | 半角終端記号（.;"?!)） |
//! | Else | その他（句読点、括弧など） |
//!
//! 分類は参照実装 `aozora2html` の Shift_JIS 正規表現
//! （`[ぁ-んゝゞ]` / `[ァ-ンーヽヾヴ]` / `[亜-熙々※仝〆〇ヶ]` 等）と 1 文字も違わないように
//! 定義してある。漢字だけは Unicode 上で 4,031 本に散らばるので、区点レンジから
//! 生成したビットマップで判定する（[`is_x0208_kanji`]）。
//!
//! 々・仝・〆・〇・ヶ を漢字とみなすのは青空文庫作業マニュアルの規定による。
//! 一方 ヵ (U+30F5) は X 0208 の仮名だが公式規定も参照実装の分類も無く、`Else` になる。

/// 文字種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharType {
    /// ひらがな（ぁ-ん、ゝ、ゞ）
    Hiragana,
    /// カタカナ（ァ-ン、ー、ヽ、ヾ、ヴ）
    Katakana,
    /// 全角英数・ギリシャ・キリル文字
    Zenkaku,
    /// 半角英数と一部記号
    Hankaku,
    /// CJK統合漢字と特殊文字
    Kanji,
    /// 半角終端記号
    HankakuTerminate,
    /// その他
    Else,
}

impl CharType {
    /// 文字種別を判定
    ///
    /// # Examples
    ///
    /// ```
    /// use aozora_core::char_type::CharType;
    ///
    /// assert_eq!(CharType::classify('あ'), CharType::Hiragana);
    /// assert_eq!(CharType::classify('ア'), CharType::Katakana);
    /// assert_eq!(CharType::classify('漢'), CharType::Kanji);
    /// assert_eq!(CharType::classify('A'), CharType::Hankaku);
    /// assert_eq!(CharType::classify('Ａ'), CharType::Zenkaku);
    /// assert_eq!(CharType::classify('.'), CharType::HankakuTerminate);
    /// assert_eq!(CharType::classify('。'), CharType::Else);
    /// ```
    pub fn classify(c: char) -> Self {
        // ひらがな: ぁ-ん (U+3041-U+3093) + ゝゞ (U+309D-U+309E)
        if matches!(c, 'ぁ'..='ん' | 'ゝ' | 'ゞ') {
            return CharType::Hiragana;
        }

        // カタカナ: ァ-ン (U+30A1-U+30F3) + ー (U+30FC) + ヽヾ (U+30FD-U+30FE) + ヴ (U+30F4)
        if matches!(c, 'ァ'..='ン' | 'ー' | 'ヽ' | 'ヾ' | 'ヴ') {
            return CharType::Katakana;
        }

        // 全角英数: ０-９ (U+FF10-U+FF19), Ａ-Ｚ (U+FF21-U+FF3A), ａ-ｚ (U+FF41-U+FF5A)
        // + ギリシャ大文字 Α-Ω (U+0391-U+03A9), 小文字 α-ω (U+03B1-U+03C9)
        // + キリル大文字 А-Я (U+0410-U+042F), 小文字 а-я (U+0430-U+044F)
        // + 記号 − (U+2212), ＆ (U+FF06), ' (U+2019), ， (U+FF0C), ． (U+FF0E)
        if matches!(c,
            '０'..='９' | 'Ａ'..='Ｚ' | 'ａ'..='ｚ' |
            'Α'..='Ω' | 'α'..='ω' |
            'А'..='Я' | 'а'..='я' |
            '−' | '＆' | '\u{2019}' | '，' | '．'
        ) {
            return CharType::Zenkaku;
        }

        // 半角英数: A-Z, a-z, 0-9, #, -, &, ', ,
        if matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '#' | '-' | '&' | '\'' | ',') {
            return CharType::Hankaku;
        }

        // 漢字: 参照実装 REGEX_KANJI = [亜-熙々※仝〆〇ヶ]（SJIS）。
        // 明示された 々 (U+3005), ※ (U+203B), 仝 (U+4EDD), 〆 (U+3006),
        // 〇 (U+3007), ヶ (U+30F6)。これらは JIS X 0208 の漢字ブロックの外にあるが、
        // 青空文庫作業マニュアルが「｜がいるかいらないかの判断にあたっては漢字と
        // みなす」と規定している（※ のみマニュアル未記載だが参照実装にある）。
        // なお ヵ (U+30F5) は規定が無く、参照実装でもどの文字種にも属さない（:else）。
        if matches!(c, '々' | '※' | '仝' | '〆' | '〇' | 'ヶ') {
            return CharType::Kanji;
        }
        // 亜-熙 = JIS X 0208 の漢字 6,355 字。X 0208 の外（NEC/IBM 拡張漢字や
        // X 0213 の第3・第4水準）は参照では :else になり、ルビ親文字の連なりが
        // そこで切れる（例:厓 は参照実装では「JIS外字」警告が出て親文字にならない）。
        if is_x0208_kanji(c) {
            return CharType::Kanji;
        }

        // 半角終端記号: . ; " ? ! )
        if matches!(c, '.' | ';' | '"' | '?' | '!' | ')') {
            return CharType::HankakuTerminate;
        }

        // その他
        CharType::Else
    }

    /// この種別がルビ親文字になれるかどうか
    ///
    /// `:else` 以外の種別はルビ親文字になれる
    pub fn can_be_ruby_base(&self) -> bool {
        !matches!(self, CharType::Else)
    }
}

/// URO（CJK統合漢字 U+4E00-U+9FFF）の下限。
const URO_START: u32 = 0x4E00;
/// URO の上限。
const URO_END: u32 = 0x9FFF;

/// URO 各符号位置が JIS X 0208 の漢字かを示すビットマップ（1 ビット / 符号位置）。
/// `build.rs` が `data/jis2ucs.json`（JIS X 0213 面区点 → Unicode）の
/// 面1 **16-01〜47-51** ＋ **48-01〜84-06** から生成する（計 6,355 字）。
static X0208_KANJI_BITS: [u8; ((URO_END - URO_START + 1) / 8) as usize] =
    include!(concat!(env!("OUT_DIR"), "/x0208_kanji_bitmap.rs"));

/// 文字が **JIS X 0208 の漢字**（第1水準＋第2水準の 6,355 字）か。
///
/// 参照実装 `REGEX_KANJI = /[亜-熙…]/`（Shift_JIS 範囲）の 亜-熙 に相当する。
/// X 0208 の漢字は Unicode 上では 4,031 本に散らばるためレンジでは表せないが、
/// 区点空間では 2 レンジで表せるので、それをビットマップに畳んで O(1) で判定する。
fn is_x0208_kanji(c: char) -> bool {
    let cp = c as u32;
    if !(URO_START..=URO_END).contains(&cp) {
        return false;
    }
    let idx = (cp - URO_START) as usize;
    X0208_KANJI_BITS[idx / 8] & (1 << (idx % 8)) != 0
}

/// 文字種別を取得する拡張トレイト
pub trait CharTypeExt {
    /// 文字種別を取得
    fn char_type(&self) -> CharType;
}

impl CharTypeExt for char {
    fn char_type(&self) -> CharType {
        CharType::classify(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧定義（検証用）: Shift_JIS へ符号化して 亜(0x889F)〜熙(0xEAA4) に入るか。
    /// ビットマップ化の前はこれが漢字判定だった。生成表がこれと完全一致することを
    /// 下のテストで保証する（一致する限り byte 一致は崩れない）。
    fn sjis_in_kanji_range_reference(c: char) -> bool {
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
    fn x0208_kanji_bitmap_matches_sjis_definition() {
        // URO 全域を総当たりし、生成したビットマップが旧定義と 1 文字も違わないことを確認する。
        let mut count = 0usize;
        for cp in URO_START..=URO_END {
            let c = char::from_u32(cp).expect("URO は全て有効なスカラ値");
            let expected = sjis_in_kanji_range_reference(c);
            assert_eq!(
                is_x0208_kanji(c),
                expected,
                "U+{cp:04X} ({c}) の判定が旧定義と食い違う"
            );
            if expected {
                count += 1;
            }
        }
        // JIS X 0208-1990/1997/2012 の漢字数（第1水準 2,965 ＋ 第2水準 3,390）。
        assert_eq!(count, 6355, "JIS X 0208 の漢字数が合わない");
    }

    #[test]
    fn x0208_kanji_excludes_extensions() {
        // 第1水準の先頭（16-01）と第2水準の末尾（84-06、X 0208-1990 で追加）は漢字。
        assert!(is_x0208_kanji('亜'));
        assert!(is_x0208_kanji('熙'));

        // NEC/IBM 拡張漢字は X 0208 の外なので漢字にしない（厓・賴）。
        assert!(!is_x0208_kanji('\u{5393}'));
        assert!(!is_x0208_kanji('\u{8CF4}'));
        assert_eq!(CharType::classify('\u{5393}'), CharType::Else);

        // 々 等は URO の外なのでビットマップ側では false。classify の明示リストが拾う。
        assert!(!is_x0208_kanji('々'));
        assert_eq!(CharType::classify('々'), CharType::Kanji);

        // ヵ (U+30F5) は青空文庫にも参照実装にも規定が無く、どの文字種にも属さない。
        assert_eq!(CharType::classify('ヵ'), CharType::Else);
    }

    #[test]
    fn test_hiragana() {
        assert_eq!(CharType::classify('あ'), CharType::Hiragana);
        assert_eq!(CharType::classify('ん'), CharType::Hiragana);
        assert_eq!(CharType::classify('ゝ'), CharType::Hiragana);
        assert_eq!(CharType::classify('ゞ'), CharType::Hiragana);
    }

    #[test]
    fn test_katakana() {
        assert_eq!(CharType::classify('ア'), CharType::Katakana);
        assert_eq!(CharType::classify('ン'), CharType::Katakana);
        assert_eq!(CharType::classify('ー'), CharType::Katakana);
        assert_eq!(CharType::classify('ヽ'), CharType::Katakana);
        assert_eq!(CharType::classify('ヾ'), CharType::Katakana);
        assert_eq!(CharType::classify('ヴ'), CharType::Katakana);
    }

    #[test]
    fn test_zenkaku() {
        assert_eq!(CharType::classify('Ａ'), CharType::Zenkaku);
        assert_eq!(CharType::classify('ａ'), CharType::Zenkaku);
        assert_eq!(CharType::classify('０'), CharType::Zenkaku);
        assert_eq!(CharType::classify('９'), CharType::Zenkaku);
        // ギリシャ文字
        assert_eq!(CharType::classify('Α'), CharType::Zenkaku);
        assert_eq!(CharType::classify('α'), CharType::Zenkaku);
        // キリル文字
        assert_eq!(CharType::classify('А'), CharType::Zenkaku);
        assert_eq!(CharType::classify('а'), CharType::Zenkaku);
    }

    #[test]
    fn test_hankaku() {
        assert_eq!(CharType::classify('A'), CharType::Hankaku);
        assert_eq!(CharType::classify('z'), CharType::Hankaku);
        assert_eq!(CharType::classify('0'), CharType::Hankaku);
        assert_eq!(CharType::classify('9'), CharType::Hankaku);
        assert_eq!(CharType::classify('#'), CharType::Hankaku);
        assert_eq!(CharType::classify('-'), CharType::Hankaku);
        assert_eq!(CharType::classify('&'), CharType::Hankaku);
        assert_eq!(CharType::classify('\''), CharType::Hankaku);
        assert_eq!(CharType::classify(','), CharType::Hankaku);
    }

    #[test]
    fn test_kanji() {
        assert_eq!(CharType::classify('漢'), CharType::Kanji);
        assert_eq!(CharType::classify('字'), CharType::Kanji);
        assert_eq!(CharType::classify('々'), CharType::Kanji);
        assert_eq!(CharType::classify('※'), CharType::Kanji);
        assert_eq!(CharType::classify('仝'), CharType::Kanji);
        assert_eq!(CharType::classify('〆'), CharType::Kanji);
        assert_eq!(CharType::classify('〇'), CharType::Kanji);
        assert_eq!(CharType::classify('ヶ'), CharType::Kanji);
    }

    #[test]
    fn test_kanji_ibm_extension_is_else() {
        // NEC/IBM 拡張漢字（SJIS が 亜-熙 の範囲外）は参照実装では :else になり、
        // ルビ親文字の連なりが切れる。厓 = SJIS 0xFA8D（U+5393）。
        // U+4E00-U+9FFF だが SJIS が 0xEAA4 超（またはエンコード不能）なので
        // Kanji ではない。
        assert_eq!(CharType::classify('\u{5393}'), CharType::Else);
        // JIS X 0208 の第2水準漢字（亜-熙 内）は従来どおり Kanji。
        assert_eq!(CharType::classify('腕'), CharType::Kanji); // 第1水準末尾付近
        assert_eq!(CharType::classify('熙'), CharType::Kanji); // 上限そのもの
    }

    #[test]
    fn test_hankaku_terminate() {
        assert_eq!(CharType::classify('.'), CharType::HankakuTerminate);
        assert_eq!(CharType::classify(';'), CharType::HankakuTerminate);
        assert_eq!(CharType::classify('"'), CharType::HankakuTerminate);
        assert_eq!(CharType::classify('?'), CharType::HankakuTerminate);
        assert_eq!(CharType::classify('!'), CharType::HankakuTerminate);
        assert_eq!(CharType::classify(')'), CharType::HankakuTerminate);
    }

    #[test]
    fn test_else() {
        assert_eq!(CharType::classify('。'), CharType::Else);
        assert_eq!(CharType::classify('、'), CharType::Else);
        assert_eq!(CharType::classify('「'), CharType::Else);
        assert_eq!(CharType::classify('」'), CharType::Else);
        assert_eq!(CharType::classify('（'), CharType::Else);
        assert_eq!(CharType::classify('）'), CharType::Else);
    }

    #[test]
    fn test_can_be_ruby_base() {
        assert!(CharType::Hiragana.can_be_ruby_base());
        assert!(CharType::Katakana.can_be_ruby_base());
        assert!(CharType::Zenkaku.can_be_ruby_base());
        assert!(CharType::Hankaku.can_be_ruby_base());
        assert!(CharType::Kanji.can_be_ruby_base());
        assert!(CharType::HankakuTerminate.can_be_ruby_base());
        assert!(!CharType::Else.can_be_ruby_base());
    }

    #[test]
    fn test_char_type_ext() {
        assert_eq!('あ'.char_type(), CharType::Hiragana);
        assert_eq!('ア'.char_type(), CharType::Katakana);
        assert_eq!('漢'.char_type(), CharType::Kanji);
    }

    #[test]
    fn test_edge_case_ke() {
        // ヶは漢字として扱う（青空文庫の指針）
        assert_eq!(CharType::classify('ヶ'), CharType::Kanji);
    }

    #[test]
    fn test_edge_case_long_vowel() {
        // 長音記号はカタカナとして扱う
        assert_eq!(CharType::classify('ー'), CharType::Katakana);
    }

    // 仕様書（02-characters.md）のテストケースを網羅

    #[test]
    fn test_spec_basic() {
        // 基本判定
        assert_eq!(CharType::classify('あ'), CharType::Hiragana);
        assert_eq!(CharType::classify('ア'), CharType::Katakana);
        assert_eq!(CharType::classify('漢'), CharType::Kanji);
        assert_eq!(CharType::classify('Ａ'), CharType::Zenkaku);
        assert_eq!(CharType::classify('A'), CharType::Hankaku);
        assert_eq!(CharType::classify('.'), CharType::HankakuTerminate);
        assert_eq!(CharType::classify('。'), CharType::Else);
    }

    #[test]
    fn test_spec_special_chars() {
        // 特殊文字
        assert_eq!(CharType::classify('々'), CharType::Kanji); // 踊り字
        assert_eq!(CharType::classify('ー'), CharType::Katakana); // 長音記号
        assert_eq!(CharType::classify('ヶ'), CharType::Kanji); // ヶは漢字扱い
        assert_eq!(CharType::classify('ゝ'), CharType::Hiragana); // ひらがな踊り字
        assert_eq!(CharType::classify('ヽ'), CharType::Katakana); // カタカナ踊り字
        assert_eq!(CharType::classify('ヴ'), CharType::Katakana); // ヴ
        assert_eq!(CharType::classify('※'), CharType::Kanji); // 米印
        assert_eq!(CharType::classify('仝'), CharType::Kanji); // 同上記号
        assert_eq!(CharType::classify('〆'), CharType::Kanji); // 締め記号
        assert_eq!(CharType::classify('〇'), CharType::Kanji); // ゼロ
    }

    #[test]
    fn test_spec_greek_cyrillic() {
        // ギリシャ・キリル文字
        assert_eq!(CharType::classify('Α'), CharType::Zenkaku); // ギリシャ大文字アルファ U+0391
        assert_eq!(CharType::classify('α'), CharType::Zenkaku); // ギリシャ小文字アルファ U+03B1
        assert_eq!(CharType::classify('Ω'), CharType::Zenkaku); // ギリシャ大文字オメガ U+03A9
        assert_eq!(CharType::classify('ω'), CharType::Zenkaku); // ギリシャ小文字オメガ U+03C9
        assert_eq!(CharType::classify('А'), CharType::Zenkaku); // キリル大文字А U+0410
        assert_eq!(CharType::classify('а'), CharType::Zenkaku); // キリル小文字а U+0430
        assert_eq!(CharType::classify('Я'), CharType::Zenkaku); // キリル大文字Я U+042F
        assert_eq!(CharType::classify('я'), CharType::Zenkaku); // キリル小文字я U+044F
    }

    #[test]
    fn test_spec_hankaku_symbols() {
        // 半角記号
        assert_eq!(CharType::classify('#'), CharType::Hankaku);
        assert_eq!(CharType::classify('-'), CharType::Hankaku);
        assert_eq!(CharType::classify('&'), CharType::Hankaku);
        assert_eq!(CharType::classify('\''), CharType::Hankaku);
        assert_eq!(CharType::classify(','), CharType::Hankaku);
        // 半角終端記号
        assert_eq!(CharType::classify('?'), CharType::HankakuTerminate);
        assert_eq!(CharType::classify('!'), CharType::HankakuTerminate);
        assert_eq!(CharType::classify(';'), CharType::HankakuTerminate);
        assert_eq!(CharType::classify('"'), CharType::HankakuTerminate);
        assert_eq!(CharType::classify(')'), CharType::HankakuTerminate);
    }

    #[test]
    fn test_spec_zenkaku_symbols() {
        // 全角記号（仕様: −＆'，．）
        assert_eq!(CharType::classify('−'), CharType::Zenkaku); // U+2212 MINUS SIGN
        assert_eq!(CharType::classify('＆'), CharType::Zenkaku); // U+FF06 FULLWIDTH AMPERSAND
        assert_eq!(CharType::classify('\u{2019}'), CharType::Zenkaku); // U+2019 RIGHT SINGLE QUOTATION MARK
        assert_eq!(CharType::classify('，'), CharType::Zenkaku); // U+FF0C FULLWIDTH COMMA
        assert_eq!(CharType::classify('．'), CharType::Zenkaku); // U+FF0E FULLWIDTH FULL STOP
    }
}
