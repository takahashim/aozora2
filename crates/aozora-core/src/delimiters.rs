//! 青空文庫形式のデリミタ定数
//!
//! 青空文庫形式で使用される全角デリミタ文字を定義

/// ルビ親文字開始 ｜ (U+FF5C)
pub const RUBY_PREFIX: char = '｜';

/// ルビ開始 《 (U+300A)
pub const RUBY_BEGIN: char = '《';

/// ルビ終了 》 (U+300B)
pub const RUBY_END: char = '》';

/// コマンド開始 ［ (U+FF3B)
pub const COMMAND_BEGIN: char = '［';

/// コマンド終了 ］ (U+FF3D)
pub const COMMAND_END: char = '］';

/// コマンド識別子 ＃ (U+FF03)
pub const IGETA: char = '＃';

/// 外字マーク ※ (U+203B)
pub const GAIJI_MARK: char = '※';

/// [`GAIJI_MARK`] の文字列版。`push_str` など `&str` が要る場所で使う
/// （両者が一致することは `test_gaiji_mark_str_matches_char` が固定する）。
pub const GAIJI_MARK_STR: &str = "※";

/// アクセント開始 〔 (U+3014)
pub const ACCENT_BEGIN: char = '〔';

/// アクセント終了 〕 (U+3015)
pub const ACCENT_END: char = '〕';

/// アクセント記号一覧。基底文字の**後ろ**に置いて1文字に畳む
/// （`e'` → é、`ae&` → æ）。どの組み合わせが有効かは記号だけでは決まらず、
/// アクセント表（`data/accent_table.json`）に載っているものだけが変換される。
///
/// | 記号 | 主な意味 |
/// |---|---|
/// | `'` | アキュートアクセント |
/// | `` ` `` | グレーブアクセント |
/// | `^` | サーカムフレックスアクセント |
/// | `~` | チルダ |
/// | `:` | ダイエレシス（ウムラウト） |
/// | `&` | リガチャ・上リング・エスツェット |
/// | `_` | マクロン |
/// | `,` | セディラ |
/// | `/` | ストローク |
/// | `@` | 逆転（`!@` → ¡） |
pub const ACCENT_MARKS: &[char] = &['\'', '`', '^', '~', ':', '&', '_', ',', '/', '@'];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delimiter_unicode_values() {
        assert_eq!(RUBY_PREFIX as u32, 0xFF5C);
        assert_eq!(RUBY_BEGIN as u32, 0x300A);
        assert_eq!(RUBY_END as u32, 0x300B);
        assert_eq!(COMMAND_BEGIN as u32, 0xFF3B);
        assert_eq!(COMMAND_END as u32, 0xFF3D);
        assert_eq!(IGETA as u32, 0xFF03);
        assert_eq!(GAIJI_MARK as u32, 0x203B);
        assert_eq!(ACCENT_BEGIN as u32, 0x3014);
        assert_eq!(ACCENT_END as u32, 0x3015);
    }

    /// 文字版と文字列版がずれないようにする。
    #[test]
    fn test_gaiji_mark_str_matches_char() {
        assert_eq!(GAIJI_MARK_STR, GAIJI_MARK.to_string());
    }
}
