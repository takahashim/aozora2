//! 装飾タイプ定義

/// 装飾の種類・記法語・レジストリを**1つの宣言**から生成する。
///
/// enum と `ALL_STYLE_TYPES` と `command_name` を別々に書くと、バリアントを
/// 足したときレジストリだけ漏れても**コンパイラもテストも気付かない**。
/// `command_name` の網羅 match が強制するのは「名前を与えること」だけで、
/// レジストリへの追加は誰も強制しないからで、round-trip テストも
/// `ALL_STYLE_TYPES` を回す以上そこに無いものは検査できない。
/// その状態でも `from_command` が黙って `None` を返すだけなので、
/// その記法は永久に認識されず注記に落ちる。3者を同時に生成して構造的に防ぐ。
macro_rules! style_types {
    ($( $(#[$meta:meta])* $variant:ident => $name:literal ),+ $(,)?) => {
        /// 装飾タイプ
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub enum StyleType {
            $( $(#[$meta])* $variant, )+
        }

        /// 全バリアントの単一レジストリ（[`style_types!`] が enum と同時に生成する）。
        const ALL_STYLE_TYPES: &[StyleType] = &[ $( StyleType::$variant, )+ ];

        impl StyleType {
            /// 正準の記法語（`［＃「…」に傍点］` の「傍点」）。
            pub fn command_name(&self) -> &'static str {
                match self {
                    $( StyleType::$variant => $name, )+
                }
            }
        }
    };
}

style_types! {
    // 傍点系（右・上）
    SesameDot => "傍点",
    WhiteSesameDot => "白ゴマ傍点",
    BlackCircle => "丸傍点",
    WhiteCircle => "白丸傍点",
    BlackTriangle => "黒三角傍点",
    WhiteTriangle => "白三角傍点",
    Bullseye => "二重丸傍点",
    Fisheye => "蛇の目傍点",
    Saltire => "ばつ傍点",

    // 傍点系（左・下）
    SesameDotAfter => "左に傍点",
    WhiteSesameDotAfter => "左に白ゴマ傍点",
    BlackCircleAfter => "左に丸傍点",
    WhiteCircleAfter => "左に白丸傍点",
    BlackTriangleAfter => "左に黒三角傍点",
    WhiteTriangleAfter => "左に白三角傍点",
    BullseyeAfter => "左に二重丸傍点",
    FisheyeAfter => "左に蛇の目傍点",
    SaltireAfter => "左にばつ傍点",

    // 傍線系（右・上）
    UnderlineSolid => "傍線",
    UnderlineDouble => "二重傍線",
    UnderlineDotted => "鎖線",
    UnderlineDashed => "破線",
    UnderlineWave => "波線",

    // 傍線系（左・下）
    OverlineSolid => "左に傍線",
    OverlineDouble => "左に二重傍線",
    OverlineDotted => "左に鎖線",
    OverlineDashed => "左に破線",
    OverlineWave => "左に波線",

    // 文字スタイル
    Bold => "太字",
    Italic => "斜体",
    Subscript => "下付き小文字",
    Superscript => "上付き小文字",
}

/// 入力別名（記法語 → 正準バリアント）。正準記法語は [`style_types!`] が持つので
/// ここには「別表記の入力」だけを置く（出力＝command_name には現れない）。
const STYLE_ALIASES: &[(&str, StyleType)] = &[
    ("行左小書き", StyleType::Subscript),
    ("行右小書き", StyleType::Superscript),
];

impl StyleType {
    /// 全バリアントの単一レジストリ
    pub fn all() -> &'static [StyleType] {
        ALL_STYLE_TYPES
    }

    /// コマンド名から装飾タイプを取得。
    /// 正準記法語は [`Self::command_name`]（[`style_types!`] が生成する唯一の
    /// 真実源）から逆引きし、別表記は [`STYLE_ALIASES`] で補う。二重表を持たない。
    pub fn from_command(command: &str) -> Option<Self> {
        ALL_STYLE_TYPES
            .iter()
            .find(|st| st.command_name() == command)
            .copied()
            .or_else(|| {
                STYLE_ALIASES
                    .iter()
                    .find(|(name, _)| *name == command)
                    .map(|(_, st)| *st)
            })
            .or_else(|| {
                // 参照 PAT_DIRECTION は `(左|下)に` を同じ方向として扱う
                // （`「あ」の下に傍点` も sesame_dot_after。実測）。
                // 正準名は `左に…` 側なので、`下に…` はそちらへ寄せて引く。
                command
                    .strip_prefix("下に")
                    .and_then(|rest| Self::from_command(&format!("左に{rest}")))
            })
    }

    /// 通常バリアントをAfterバリアントに変換（左側表示用）
    pub fn to_after_variant(self) -> Self {
        match self {
            // 傍点系
            StyleType::SesameDot => StyleType::SesameDotAfter,
            StyleType::WhiteSesameDot => StyleType::WhiteSesameDotAfter,
            StyleType::BlackCircle => StyleType::BlackCircleAfter,
            StyleType::WhiteCircle => StyleType::WhiteCircleAfter,
            StyleType::BlackTriangle => StyleType::BlackTriangleAfter,
            StyleType::WhiteTriangle => StyleType::WhiteTriangleAfter,
            StyleType::Bullseye => StyleType::BullseyeAfter,
            StyleType::Fisheye => StyleType::FisheyeAfter,
            StyleType::Saltire => StyleType::SaltireAfter,
            // 傍線系
            StyleType::UnderlineSolid => StyleType::OverlineSolid,
            StyleType::UnderlineDouble => StyleType::OverlineDouble,
            StyleType::UnderlineDotted => StyleType::OverlineDotted,
            StyleType::UnderlineDashed => StyleType::OverlineDashed,
            StyleType::UnderlineWave => StyleType::OverlineWave,
            // 既にAfterバリアントの場合はそのまま
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_type_from_command() {
        assert_eq!(StyleType::from_command("傍点"), Some(StyleType::SesameDot));
        assert_eq!(StyleType::from_command("太字"), Some(StyleType::Bold));
        assert_eq!(StyleType::from_command("未知"), None);
    }

    /// 記法語 → バリアント → 記法語 が一巡することを固定する。
    ///
    /// 「レジストリに全バリアントが載っていること」はこのテストでは保証できない
    /// （`ALL_STYLE_TYPES` を回す以上、そこに無いものは検査対象にならない）。
    /// それは [`style_types!`] が enum とレジストリを同時に生成することで
    /// 構造的に保証している。ここが見るのは記法語の対応そのもの。
    #[test]
    fn test_command_names_round_trip() {
        for st in StyleType::all() {
            let name = st.command_name();
            assert_eq!(
                StyleType::from_command(name),
                Some(*st),
                "command_name()={name:?} が from_command で {st:?} に戻らない（レジストリ漏れ）"
            );
        }
    }

    #[test]
    fn test_style_aliases_resolve() {
        assert_eq!(
            StyleType::from_command("行左小書き"),
            Some(StyleType::Subscript)
        );
        assert_eq!(
            StyleType::from_command("行右小書き"),
            Some(StyleType::Superscript)
        );
    }
}
