//! HTMLレンダラー
//!
//! ASTノードをHTMLに変換します。

use crate::document::{
    extract_after_text_lines, extract_bibliographical_lines, extract_body_lines,
    extract_header_info,
};
use crate::node::Node;

use super::block_manager::BlockManager;
use super::document_renderer::DocumentRenderer;
use super::line_processor::LineProcessor;
use super::node_renderer::NodeRenderer;
use super::options::RenderOptions;
use super::presentation::LineType;
use super::section_renderer::SectionRenderer;

/// HTMLレンダラー
#[derive(Debug, Clone)]
pub struct HtmlRenderer {
    options: RenderOptions,
}

impl HtmlRenderer {
    /// 新しいレンダラーを作成
    pub fn new(options: RenderOptions) -> Self {
        Self { options }
    }

    /// テキスト全体をHTMLに変換
    pub fn render(&mut self, input: &str) -> String {
        let mut output = String::new();
        let lines: Vec<&str> = input.lines().collect();

        // ヘッダー情報を抽出
        let header_info = extract_header_info(&lines);

        // サブレンダラーを作成
        let doc_renderer = DocumentRenderer::new(&self.options);
        let mut node_renderer = NodeRenderer::new(&self.options);
        let mut block_manager = BlockManager::new();

        // HTMLヘッダーとメタデータセクションを出力
        doc_renderer.render_html_head(&mut output, &header_info);
        doc_renderer.render_metadata_section(&mut output, &header_info);

        // main_text開始
        doc_renderer.render_main_text_start(&mut output);

        // 本文をレンダリング
        self.render_body(&mut output, &lines, &mut node_renderer, &mut block_manager);

        // main_text終了
        doc_renderer.render_main_text_end(&mut output);

        // 本文終わり後のテキスト（after_text）セクション
        let after_text_lines = extract_after_text_lines(&lines);
        SectionRenderer::render_after_text(
            &mut output,
            &after_text_lines,
            &doc_renderer,
            &mut node_renderer,
            &mut block_manager,
        );

        // 底本情報（bibliographical_information）セクション
        let biblio_lines = extract_bibliographical_lines(&lines);
        SectionRenderer::render_bibliographical(
            &mut output,
            &biblio_lines,
            &doc_renderer,
            &mut node_renderer,
            &mut block_manager,
        );

        // 表記について（notation_notes）セクション
        doc_renderer.render_notation_notes(
            &mut output,
            node_renderer.has_notes(),
            node_renderer.has_jisx0213(),
            node_renderer.has_accent(),
            node_renderer.unconverted_gaiji(),
        );

        // 図書カードセクション
        doc_renderer.render_card_section(&mut output);

        doc_renderer.render_html_foot(&mut output);

        output
    }

    /// 本文のレンダリング
    fn render_body(
        &self,
        output: &mut String,
        lines: &[&str],
        node_renderer: &mut NodeRenderer,
        block_manager: &mut BlockManager,
    ) {
        let body_lines = extract_body_lines(lines);

        for line in &body_lines {
            let result = LineProcessor::render_line(line, node_renderer, block_manager);

            // ぶら下げブロック内の場合
            if let Some((wrap_width, text_indent)) = result.burasage_context {
                if result.line_type == LineType::Inline {
                    output.push_str(&LineProcessor::wrap_burasage_line(
                        &result.html,
                        wrap_width,
                        text_indent,
                    ));
                    continue;
                }
            }

            // 通常の行処理
            LineProcessor::finalize_line(output, &result.html, line, block_manager);
        }

        // 閉じられていないブロックを閉じる
        LineProcessor::close_all_blocks(output, block_manager);
    }

    /// 1行をHTMLに変換（公開API）
    pub fn render_line(&mut self, line: &str) -> String {
        let mut node_renderer = NodeRenderer::new(&self.options);
        let mut block_manager = BlockManager::new();
        let result = LineProcessor::render_line(line, &mut node_renderer, &mut block_manager);
        result.html
    }

    /// ノード列をHTMLに変換
    pub fn render_nodes(&mut self, nodes: &[Node]) -> String {
        let mut node_renderer = NodeRenderer::new(&self.options);
        let mut block_manager = BlockManager::new();
        node_renderer.render_nodes(nodes, &mut block_manager)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_text() {
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render_line("こんにちは");
        assert_eq!(html, "こんにちは");
    }

    #[test]
    fn test_render_ruby() {
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render_line("漢字《かんじ》");
        assert!(html.contains("<ruby>"));
        assert!(html.contains("<rb>漢字</rb>"));
        assert!(html.contains("<rt>かんじ</rt>"));
    }
}
