//! HTML変換モジュール
//!
//! 青空文庫形式のテキストをHTMLに変換します。

mod options;
mod renderer;

pub use options::RenderOptions;
pub use renderer::HtmlRenderer;

/// 青空文庫形式のテキストをHTMLに変換
///
/// # Arguments
///
/// * `input` - 青空文庫形式のテキスト
/// * `options` - レンダリングオプション
///
/// # Returns
///
/// HTML文字列
///
/// # Examples
///
/// ```
/// use aozora2::html::{convert, RenderOptions};
///
/// let input = "吾輩《わがはい》は猫である";
/// let html = convert(input, &RenderOptions::default());
/// assert!(html.contains("<ruby>"));
/// ```
pub fn convert(input: &str, options: &RenderOptions) -> String {
    let mut renderer = HtmlRenderer::new(options.clone());
    renderer.render(input)
}

/// 1行をHTMLに変換
pub fn convert_line(line: &str, options: &RenderOptions) -> String {
    let mut renderer = HtmlRenderer::new(options.clone());
    renderer.render_line(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_simple() {
        let html = convert("こんにちは", &RenderOptions::default());
        assert!(html.contains("こんにちは"));
    }

    #[test]
    fn test_convert_ruby() {
        let html = convert("漢字《かんじ》", &RenderOptions::default());
        assert!(html.contains("<ruby>"));
        assert!(html.contains("漢字"));
        assert!(html.contains("かんじ"));
    }

    #[test]
    fn test_convert_line() {
        let html = convert_line("猫《ねこ》", &RenderOptions::default());
        assert!(html.contains("<ruby>"));
    }
}
