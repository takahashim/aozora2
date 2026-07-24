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

impl StyleType {
    /// コマンド名から装飾タイプを取得
    pub fn from_command(command: &str) -> Option<Self> {
        match command {
            // 傍点系（右・上）
            "傍点" => Some(StyleType::SesameDot),
            "白ゴマ傍点" => Some(StyleType::WhiteSesameDot),
            "丸傍点" => Some(StyleType::BlackCircle),
            "白丸傍点" => Some(StyleType::WhiteCircle),
            "黒三角傍点" => Some(StyleType::BlackTriangle),
            "白三角傍点" => Some(StyleType::WhiteTriangle),
            "二重丸傍点" => Some(StyleType::Bullseye),
            "蛇の目傍点" => Some(StyleType::Fisheye),
            "ばつ傍点" => Some(StyleType::Saltire),
            // 傍点系（左・下）
            "左に傍点" => Some(StyleType::SesameDotAfter),
            "左に白ゴマ傍点" => Some(StyleType::WhiteSesameDotAfter),
            "左に丸傍点" => Some(StyleType::BlackCircleAfter),
            "左に白丸傍点" => Some(StyleType::WhiteCircleAfter),
            "左に黒三角傍点" => Some(StyleType::BlackTriangleAfter),
            "左に白三角傍点" => Some(StyleType::WhiteTriangleAfter),
            "左に二重丸傍点" => Some(StyleType::BullseyeAfter),
            "左に蛇の目傍点" => Some(StyleType::FisheyeAfter),
            "左にばつ傍点" => Some(StyleType::SaltireAfter),
            // 傍線系（右・上）
            "傍線" => Some(StyleType::UnderlineSolid),
            "二重傍線" => Some(StyleType::UnderlineDouble),
            "鎖線" => Some(StyleType::UnderlineDotted),
            "破線" => Some(StyleType::UnderlineDashed),
            "波線" => Some(StyleType::UnderlineWave),
            // 傍線系（左・下）
            "左に傍線" => Some(StyleType::OverlineSolid),
            "左に二重傍線" => Some(StyleType::OverlineDouble),
            "左に鎖線" => Some(StyleType::OverlineDotted),
            "左に破線" => Some(StyleType::OverlineDashed),
            "左に波線" => Some(StyleType::OverlineWave),
            // 文字スタイル
            "太字" => Some(StyleType::Bold),
            "斜体" => Some(StyleType::Italic),
            "下付き小文字" | "行左小書き" => Some(StyleType::Subscript),
            "上付き小文字" | "行右小書き" => Some(StyleType::Superscript),
            _ => None,
        }
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

    /// from_command と command_name は同じ「記法語→バリアント」対応を
    /// 二重に書いている。片方だけ直すとずれるので、全バリアントで
    /// command_name → from_command の往復が恒等になることを縛る。
    /// （後日データ駆動化するまでの二重記述ドリフト検出）
    #[test]
    fn test_command_name_round_trips_through_from_command() {
        // 全バリアントを列挙する。新バリアントを追加したらここにも足すこと
        // （足し忘れても、from_command 側の記法語をここに書かねばならないので
        //   結局この配列が全記法語の単一台帳になる）。
        let all = [
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
        for st in all {
            let name = st.command_name();
            assert_eq!(
                StyleType::from_command(name),
                Some(st),
                "command_name()={name:?} が from_command で {st:?} に戻らない（二重表がずれている）"
            );
        }
    }
}
