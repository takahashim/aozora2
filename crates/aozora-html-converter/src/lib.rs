//! aozora-html-converter: 青空文庫形式をHTMLに変換
//!
//! このクレートは青空文庫形式のテキストをHTMLに変換する機能を提供します。
//!
//! # 基本的な使い方
//!
//! ```
//! use aozora_html_converter::{convert, RenderOptions};
//!
//! let input = "吾輩《わがはい》は猫である";
//! let html = convert(input, &RenderOptions::default());
//! assert!(html.contains("<ruby>"));
//! ```

pub mod options;
pub mod renderer;

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

    #[test]
    fn test_convert_bouten() {
        let html = convert_line("重要だ［＃「重要」に傍点］", &RenderOptions::default());
        assert!(html.contains("重要"));
    }

    #[test]
    fn test_convert_midashi() {
        let html = convert_line(
            "第一章［＃「第一章」は大見出し］",
            &RenderOptions::default(),
        );
        assert!(html.contains("第一章"));
        assert!(html.contains("o-midashi"));
    }

    #[test]
    fn test_convert_jisage() {
        let html = convert_line("［＃ここから2字下げ］", &RenderOptions::default());
        assert!(html.contains("jisage_2"));
    }

    #[test]
    fn test_convert_gaiji() {
        let html = convert_line("※［＃「丸印」、U+25CB］", &RenderOptions::default());
        assert!(html.contains("○"));
    }
}
