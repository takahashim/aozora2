//! レンダリング状態管理
//!
//! HTMLレンダリング中の状態（外字使用、注記使用など）を追跡します。

/// 未変換外字情報
#[derive(Debug, Clone)]
pub struct UnconvertedGaiji {
    /// 外字名（説明の最後の「、」より前の部分）
    pub gaiji_name: String,
    /// ページ-行数（説明の最後の「、」より後の部分）
    pub page_line: String,
}

/// レンダリング状態
///
/// HTMLレンダリング中に蓄積される情報を保持します。
/// この情報は後でnotation_notes（表記について）セクションの
/// 生成に使用されます。
#[derive(Debug, Default)]
pub struct RenderingState {
    /// 注記を使用したかどうか
    pub has_notes: bool,
    /// 外字画像を使用したかどうか
    pub has_gaiji_images: bool,
    /// アクセント記号を使用したかどうか
    pub has_accent: bool,
    /// JIS X 0213文字を使用したかどうか
    pub has_jisx0213: bool,
    /// 未変換外字のリスト
    pub unconverted_gaiji: Vec<UnconvertedGaiji>,
}

impl RenderingState {
    /// 新しいレンダリング状態を作成
    pub fn new() -> Self {
        Self::default()
    }

    /// 未変換外字を追加（重複を避ける）
    pub fn add_unconverted_gaiji(&mut self, description: &str) {
        // descriptionを最後の「、」で分解（外字説明とページ-行数を分離）
        let (gaiji_name, page_line) = if let Some(last_comma_pos) = description.rfind('、') {
            let name = &description[..last_comma_pos];
            let line = &description[last_comma_pos + '、'.len_utf8()..];
            (name.to_string(), line.to_string())
        } else {
            (description.to_string(), String::new())
        };

        // 既に追加済みの場合はスキップ
        if self
            .unconverted_gaiji
            .iter()
            .any(|g| g.gaiji_name == gaiji_name)
        {
            return;
        }
        self.unconverted_gaiji.push(UnconvertedGaiji {
            gaiji_name,
            page_line,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_unconverted_gaiji() {
        let mut state = RenderingState::new();
        state.add_unconverted_gaiji("「てへん＋夸」、37-下-12");

        assert_eq!(state.unconverted_gaiji.len(), 1);
        assert_eq!(state.unconverted_gaiji[0].gaiji_name, "「てへん＋夸」");
        assert_eq!(state.unconverted_gaiji[0].page_line, "37-下-12");
    }

    #[test]
    fn test_add_unconverted_gaiji_no_duplicate() {
        let mut state = RenderingState::new();
        state.add_unconverted_gaiji("「てへん＋夸」、37-下-12");
        state.add_unconverted_gaiji("「てへん＋夸」、38-上-5");

        assert_eq!(state.unconverted_gaiji.len(), 1);
    }

    #[test]
    fn test_add_unconverted_gaiji_no_comma() {
        let mut state = RenderingState::new();
        state.add_unconverted_gaiji("「にんべん＋燮のつくり」");

        assert_eq!(state.unconverted_gaiji.len(), 1);
        assert_eq!(
            state.unconverted_gaiji[0].gaiji_name,
            "「にんべん＋燮のつくり」"
        );
        assert_eq!(state.unconverted_gaiji[0].page_line, "");
    }
}
