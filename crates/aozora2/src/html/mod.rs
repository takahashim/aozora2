//! HTML変換モジュール
//!
//! 青空文庫形式のテキストをHTMLに変換します。

mod block_manager;
mod document_renderer;
mod node_renderer;
mod options;
mod presentation;
mod renderer;
mod tag_generator;

pub use options::RenderOptions;
pub use presentation::html_escape;
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
/// // 青空文庫形式: ヘッダー、空行、本文
/// let input = "タイトル\n\n吾輩《わがはい》は猫である";
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
        // 青空文庫形式: ヘッダー、空行、本文の構造
        let input = "タイトル\n\nこんにちは";
        let html = convert(input, &RenderOptions::default());
        assert!(html.contains("こんにちは"));
        assert!(html.contains("<!DOCTYPE"));
    }

    #[test]
    fn test_convert_ruby() {
        // 青空文庫形式: ヘッダー、空行、本文の構造
        let input = "タイトル\n\n漢字《かんじ》";
        let html = convert(input, &RenderOptions::default());
        assert!(html.contains("<ruby>"));
        assert!(html.contains("漢字"));
        assert!(html.contains("かんじ"));
    }

    #[test]
    fn test_convert_line() {
        let html = convert_line("猫《ねこ》", &RenderOptions::default());
        assert!(html.contains("<ruby>"));
    }

    /// くの字点は文字をそのまま出力し、「表記について」に注記を足す
    #[test]
    fn test_kunoji_note() {
        let html = convert("タイトル\n\n本文でわざ／＼と使う", &RenderOptions::default());
        assert!(html.contains("<li>「くの字点」は「／＼」で表しました。</li>"));
        assert!(!html.contains("濁点付きくの字点"));
    }

    #[test]
    fn test_dakuten_kunoji_note() {
        let html = convert("タイトル\n\n本文でしみ／″＼と使う", &RenderOptions::default());
        assert!(html.contains("<li>「濁点付きくの字点」は「／″＼」で表しました。</li>"));
    }

    /// 両方使った場合は 1 行にまとめる
    #[test]
    fn test_both_kunoji_notes_are_combined() {
        let html = convert(
            "タイトル\n\nわざ／＼としみ／″＼と使う",
            &RenderOptions::default(),
        );
        assert!(html.contains(
            "<li>「くの字点」は「／＼」で、「濁点付きくの字点」は「／″＼」で表しました。</li>"
        ));
    }

    /// くの字点を使っていなければ注記は出さない
    #[test]
    fn test_no_kunoji_note_without_kunoji() {
        let html = convert("タイトル\n\n／だけ、＼だけ", &RenderOptions::default());
        assert!(!html.contains("くの字点"));
    }
}
