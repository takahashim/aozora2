//! セクションレンダラー
//!
//! 本文終わり後のテキスト（after_text）や底本情報（bibliographical_information）
//! などのセクションをレンダリングします。

use super::block_manager::BlockManager;
use super::document_renderer::DocumentRenderer;
use super::line_processor::LineProcessor;
use super::node_renderer::NodeRenderer;
use super::presentation::auto_link;

/// セクションレンダラー
pub struct SectionRenderer;

impl SectionRenderer {
    /// after_textセクションをレンダリング
    pub fn render_after_text(
        output: &mut String,
        lines: &[&str],
        doc_renderer: &DocumentRenderer,
        node_renderer: &mut NodeRenderer,
        block_manager: &mut BlockManager,
    ) {
        if lines.is_empty() {
            return;
        }

        doc_renderer.render_after_text_header(output);
        for line in lines {
            let result = LineProcessor::render_line(line, node_renderer, block_manager);
            // 自動リンク化を適用
            let line_html = auto_link(&result.html);
            output.push_str(&line_html);
            output.push_str("<br />\r\n");
        }
        doc_renderer.render_after_text_footer(output);
    }

    /// bibliographical_informationセクションをレンダリング
    pub fn render_bibliographical(
        output: &mut String,
        lines: &[&str],
        doc_renderer: &DocumentRenderer,
        node_renderer: &mut NodeRenderer,
        block_manager: &mut BlockManager,
    ) {
        if lines.is_empty() {
            return;
        }

        doc_renderer.render_bibliographical_header(output);
        for line in lines {
            let result = LineProcessor::render_line(line, node_renderer, block_manager);
            // 自動リンク化を適用
            let line_html = auto_link(&result.html);
            output.push_str(&line_html);
            output.push_str("<br />\r\n");
        }
        doc_renderer.render_bibliographical_footer(output);
    }
}
