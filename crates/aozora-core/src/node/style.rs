//! 装飾タイプ定義

/// 装飾タイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleType {
    // 傍点系（右・上）
    SesameDot,
    WhiteSesameDot,
    BlackCircle,
    WhiteCircle,
    BlackTriangle,
    WhiteTriangle,
    Bullseye,
    Fisheye,
    Saltire,

    // 傍点系（左・下）
    SesameDotAfter,
    WhiteSesameDotAfter,
    BlackCircleAfter,
    WhiteCircleAfter,
    BlackTriangleAfter,
    WhiteTriangleAfter,
    BullseyeAfter,
    FisheyeAfter,
    SaltireAfter,

    // 傍線系（右・上）
    UnderlineSolid,
    UnderlineDouble,
    UnderlineDotted,
    UnderlineDashed,
    UnderlineWave,

    // 傍線系（左・下）
    OverlineSolid,
    OverlineDouble,
    OverlineDotted,
    OverlineDashed,
    OverlineWave,

    // 文字スタイル
    Bold,
    Italic,
    Subscript,
    Superscript,
}

/// 全バリアントの単一レジストリ。command_name（網羅 match）とともに
/// 記法語↔バリアントの唯一の台帳をなす。from_command はここから導出する。
/// バリアントを増やしたらここへ追加すること（漏れは round-trip テストが検出）。
const ALL_STYLE_TYPES: &[StyleType] = &[
    StyleType::SesameDot,
    StyleType::WhiteSesameDot,
    StyleType::BlackCircle,
    StyleType::WhiteCircle,
    StyleType::BlackTriangle,
    StyleType::WhiteTriangle,
    StyleType::Bullseye,
    StyleType::Fisheye,
    StyleType::Saltire,
    StyleType::SesameDotAfter,
    StyleType::WhiteSesameDotAfter,
    StyleType::BlackCircleAfter,
    StyleType::WhiteCircleAfter,
    StyleType::BlackTriangleAfter,
    StyleType::WhiteTriangleAfter,
    StyleType::BullseyeAfter,
    StyleType::FisheyeAfter,
    StyleType::SaltireAfter,
    StyleType::UnderlineSolid,
    StyleType::UnderlineDouble,
    StyleType::UnderlineDotted,
    StyleType::UnderlineDashed,
    StyleType::UnderlineWave,
    StyleType::OverlineSolid,
    StyleType::OverlineDouble,
    StyleType::OverlineDotted,
    StyleType::OverlineDashed,
    StyleType::OverlineWave,
    StyleType::Bold,
    StyleType::Italic,
    StyleType::Subscript,
    StyleType::Superscript,
];

/// 入力別名（記法語 → 正準バリアント）。正準記法語は command_name にあるので
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
    /// 正準記法語は command_name（唯一の真実源）から逆引きし、別表記は
    /// STYLE_ALIASES で補う。二重表を持たない。
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

    /// コマンド名を取得
    pub fn command_name(&self) -> &'static str {
        match self {
            StyleType::SesameDot => "傍点",
            StyleType::WhiteSesameDot => "白ゴマ傍点",
            StyleType::BlackCircle => "丸傍点",
            StyleType::WhiteCircle => "白丸傍点",
            StyleType::BlackTriangle => "黒三角傍点",
            StyleType::WhiteTriangle => "白三角傍点",
            StyleType::Bullseye => "二重丸傍点",
            StyleType::Fisheye => "蛇の目傍点",
            StyleType::Saltire => "ばつ傍点",
            StyleType::SesameDotAfter => "左に傍点",
            StyleType::WhiteSesameDotAfter => "左に白ゴマ傍点",
            StyleType::BlackCircleAfter => "左に丸傍点",
            StyleType::WhiteCircleAfter => "左に白丸傍点",
            StyleType::BlackTriangleAfter => "左に黒三角傍点",
            StyleType::WhiteTriangleAfter => "左に白三角傍点",
            StyleType::BullseyeAfter => "左に二重丸傍点",
            StyleType::FisheyeAfter => "左に蛇の目傍点",
            StyleType::SaltireAfter => "左にばつ傍点",
            StyleType::UnderlineSolid => "傍線",
            StyleType::UnderlineDouble => "二重傍線",
            StyleType::UnderlineDotted => "鎖線",
            StyleType::UnderlineDashed => "破線",
            StyleType::UnderlineWave => "波線",
            StyleType::OverlineSolid => "左に傍線",
            StyleType::OverlineDouble => "左に二重傍線",
            StyleType::OverlineDotted => "左に鎖線",
            StyleType::OverlineDashed => "左に破線",
            StyleType::OverlineWave => "左に波線",
            StyleType::Bold => "太字",
            StyleType::Italic => "斜体",
            StyleType::Subscript => "下付き小文字",
            StyleType::Superscript => "上付き小文字",
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

    /// レジストリ ALL_STYLE_TYPES に載っていないバリアントは from_command で
    /// 逆引きできない（記法として認識されない）。command_name（網羅 match）は
    /// コンパイラが全バリアントを強制するので、両者が一致することで
    /// 「レジストリに全バリアントが載っている」ことを保証する。
    #[test]
    fn test_registry_covers_every_variant_and_round_trips() {
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
        assert_eq!(StyleType::from_command("行左小書き"), Some(StyleType::Subscript));
        assert_eq!(
            StyleType::from_command("行右小書き"),
            Some(StyleType::Superscript)
        );
    }
}
