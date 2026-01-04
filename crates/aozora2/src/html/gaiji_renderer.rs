//! 外字レンダラー
//!
//! 外字（Gaiji）とアクセント記号をHTMLに変換します。

use aozora_core::gaiji::{parse_gaiji, GaijiResult};

use super::options::RenderOptions;
use super::presentation::{html_escape, jis_code_to_path};
use super::rendering_state::RenderingState;

/// 外字レンダラー
pub struct GaijiRenderer<'a> {
    options: &'a RenderOptions,
}

impl<'a> GaijiRenderer<'a> {
    /// 新しい外字レンダラーを作成
    pub fn new(options: &'a RenderOptions) -> Self {
        Self { options }
    }

    /// 外字をHTMLに変換
    pub fn render_gaiji(
        &self,
        description: &str,
        unicode: Option<&str>,
        jis_code: Option<&str>,
        state: &mut RenderingState,
    ) -> String {
        match (unicode, jis_code) {
            // JisConverted: unicodeとjis_code両方がある場合
            (Some(u), Some(jis)) => {
                state.has_jisx0213 = true;
                if self.options.use_jisx0213 || self.options.use_unicode {
                    return u.chars().map(|c| format!("&#{};", c as u32)).collect();
                } else {
                    state.has_gaiji_images = true;
                    let (folder, file) = jis_code_to_path(jis);
                    return format!(
                        "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                        self.options.gaiji_dir,
                        folder,
                        file,
                        html_escape(description)
                    );
                }
            }
            // Unicode: unicodeだけがある場合（JISコードがない）
            (Some(u), None) => {
                if self.options.use_unicode {
                    return u.chars().map(|c| format!("&#{};", c as u32)).collect();
                }
                // JISコードがないので画像化できない → 注記として出力
                state.has_notes = true;
                state.add_unconverted_gaiji(description);
                return format!(
                    "※<span class=\"notes\">［＃{}］</span>",
                    html_escape(description)
                );
            }
            // JisImage: jis_codeだけがある場合
            (None, Some(jis)) => {
                state.has_gaiji_images = true;
                let (folder, file) = jis_code_to_path(jis);
                return format!(
                    "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                    self.options.gaiji_dir,
                    folder,
                    file,
                    html_escape(description)
                );
            }
            // 両方Noneの場合は再度パース
            (None, None) => {}
        }

        self.render_gaiji_from_parse(description, state)
    }

    /// descriptionから外字をパースして変換
    fn render_gaiji_from_parse(&self, description: &str, state: &mut RenderingState) -> String {
        match parse_gaiji(description) {
            GaijiResult::Unicode(s) => {
                if self.options.use_unicode {
                    s.chars().map(|c| format!("&#{};", c as u32)).collect()
                } else {
                    state.has_notes = true;
                    state.add_unconverted_gaiji(description);
                    format!(
                        "※<span class=\"notes\">［＃{}］</span>",
                        html_escape(description)
                    )
                }
            }
            GaijiResult::JisConverted {
                jis_code: jis,
                unicode: u,
            } => {
                state.has_jisx0213 = true;
                if self.options.use_jisx0213 || self.options.use_unicode {
                    u.chars().map(|c| format!("&#{};", c as u32)).collect()
                } else {
                    state.has_gaiji_images = true;
                    let (folder, file) = jis_code_to_path(&jis);
                    format!(
                        "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                        self.options.gaiji_dir,
                        folder,
                        file,
                        html_escape(description)
                    )
                }
            }
            GaijiResult::JisImage { jis_code: jis } => {
                state.has_gaiji_images = true;
                let (folder, file) = jis_code_to_path(&jis);
                format!(
                    "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                    self.options.gaiji_dir,
                    folder,
                    file,
                    html_escape(description)
                )
            }
            GaijiResult::Unconvertible => {
                state.has_notes = true;
                state.add_unconverted_gaiji(description);
                format!(
                    "※<span class=\"notes\">［＃{}］</span>",
                    html_escape(description)
                )
            }
        }
    }

    /// アクセント記号をHTMLに変換
    pub fn render_accent(
        &self,
        code: &str,
        name: &str,
        unicode: Option<&str>,
        state: &mut RenderingState,
    ) -> String {
        state.has_accent = true;
        if self.options.use_jisx0213 || self.options.use_unicode {
            if let Some(u) = unicode {
                u.chars().map(|c| format!("&#{};", c as u32)).collect()
            } else {
                String::new()
            }
        } else {
            state.has_gaiji_images = true;
            let (folder, file) = jis_code_to_path(code);
            format!(
                "<img src=\"{}{}/{}.png\" alt=\"※({})\" class=\"gaiji\" />",
                self.options.gaiji_dir,
                folder,
                file,
                html_escape(name)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_gaiji_unicode() {
        let options = RenderOptions {
            use_unicode: true,
            ..Default::default()
        };
        let renderer = GaijiRenderer::new(&options);
        let mut state = RenderingState::new();

        let result = renderer.render_gaiji("「丸印」、U+25CB", Some("○"), None, &mut state);
        assert_eq!(result, "&#9675;"); // ○ = U+25CB = 9675
    }

    #[test]
    fn test_render_gaiji_jis_image() {
        let options = RenderOptions {
            use_jisx0213: false,
            use_unicode: false,
            ..Default::default()
        };
        let renderer = GaijiRenderer::new(&options);
        let mut state = RenderingState::new();

        let result =
            renderer.render_gaiji("「二の字点」、1-02-22", Some("〻"), Some("1-02-22"), &mut state);
        assert!(result.contains("<img"));
        assert!(result.contains("1-02/1-02-22.png"));
        assert!(state.has_gaiji_images);
    }

    #[test]
    fn test_render_gaiji_unconvertible() {
        let options = RenderOptions::default();
        let renderer = GaijiRenderer::new(&options);
        let mut state = RenderingState::new();

        let result = renderer.render_gaiji("「てへん＋夸」、37-下-12", None, None, &mut state);
        assert!(result.contains("※<span class=\"notes\">"));
        assert!(state.has_notes);
        assert_eq!(state.unconverted_gaiji.len(), 1);
    }

    #[test]
    fn test_render_accent() {
        let options = RenderOptions {
            use_unicode: true,
            ..Default::default()
        };
        let renderer = GaijiRenderer::new(&options);
        let mut state = RenderingState::new();

        let result = renderer.render_accent("1-09-01", "アクセント付きA", Some("Á"), &mut state);
        assert_eq!(result, "&#193;"); // Á = 193
        assert!(state.has_accent);
    }
}
