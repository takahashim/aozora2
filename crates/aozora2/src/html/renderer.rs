//! HTMLレンダラー
//!
//! ASTノードをHTMLに変換します。

use aozora_core::document::{
    extract_after_text_lines, extract_bibliographical_lines, extract_body_lines,
    extract_header_info,
};
use aozora_core::node::{BlockParams, BlockType, Node};
use aozora_core::parser::parse;
use aozora_core::parser::reference_resolver::resolve_inline_ruby;
use aozora_core::tokenizer::tokenize;

use super::block_manager::BlockManager;
use super::document_renderer::DocumentRenderer;
use super::node_renderer::NodeRenderer;
use super::options::RenderOptions;
use super::presentation::{auto_link, classify_line, is_block_only_line, LineType};

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
        // 参照実装 aozora2html の Jstream は CRLF だけを行の区切りとみなし、
        // 単独の LF は本文の文字として扱う。lines() は LF でも分割してしまう。
        let mut lines: Vec<&str> = input.split("\r\n").collect();
        if lines.last() == Some(&"") {
            lines.pop();
        }

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

        // 本文のみ抽出してレンダリング
        let body_lines = extract_body_lines(&lines);
        for line in &body_lines {
            let (line_html, has_explicit_close) =
                self.render_line_with_context(line, &mut node_renderer, &mut block_manager);

            // ぶら下げブロック内かどうかをチェック
            let burasage_ctx = block_manager.find_burasage_context();
            let line_type = classify_line(&line_html);

            if let Some((wrap_width, text_indent)) = burasage_ctx {
                // ぶら下げブロック内: インライン行を個別のdivでラップ
                if line_type == LineType::Inline {
                    output.push_str(&format!(
                        "<div class=\"burasage\" style=\"margin-left: {wrap_width}em; text-indent: {text_indent}em;\">{line_html}</div>"
                    ));
                    output.push_str("\r\n");
                    continue;
                }
            }

            // line_htmlが空でかつ元の行も空じゃない場合（コマンドのみの行）は何も出力しない
            if line_html.is_empty() && !line.is_empty() {
                continue;
            }

            output.push_str(&line_html);

            // インラインブロック（is_block = false）は行末で閉じる
            let closed_blocks = block_manager.close_inline_blocks();
            for (block_type, params) in closed_blocks {
                output.push_str(&block_manager.render_block_end_tag(&block_type, &params));
            }

            // ブロック開始/終了だけの行（div終わる）には<br />を追加しない
            let ends_with_div = output.ends_with("</div>");

            let needs_br = if line_html.is_empty() {
                // line_htmlが空の場合：元の行が空白行なら<br />を追加
                true
            } else if has_explicit_close {
                // ［＃ここで…終わり］の行は @terprip=false で行末 <br /> を出さない
                false
            } else if ends_with_div {
                // 現在の出力がdiv終了タグで終わる場合は<br />不要
                false
            } else {
                !is_block_only_line(&line_html)
            };
            if needs_br {
                output.push_str("<br />");
            }
            output.push_str("\r\n");
        }

        // 閉じられていないブロックを閉じる
        while let Some(ctx) = block_manager.pop() {
            output.push_str(&block_manager.render_block_end_tag(&ctx.block_type, &ctx.params));
        }

        // main_text終了
        doc_renderer.render_main_text_end(&mut output);

        // ここから先は参照実装の tail_output に相当するセクション
        node_renderer.enter_tail();

        // 本文終わり後のテキスト（after_text）セクション
        let after_text_lines = extract_after_text_lines(&lines);
        if !after_text_lines.is_empty() {
            doc_renderer.render_after_text_header(&mut output);
            for line in &after_text_lines {
                let line_html = self
                    .render_line_with_context(line, &mut node_renderer, &mut block_manager)
                    .0;
                // 自動リンク化を適用
                let line_html = auto_link(&line_html);
                output.push_str(&line_html);
                output.push_str("<br />\r\n");
            }
            doc_renderer.render_after_text_footer(&mut output);
        }

        // 底本情報（bibliographical_information）セクション
        let biblio_lines = extract_bibliographical_lines(&lines);
        if !biblio_lines.is_empty() {
            doc_renderer.render_bibliographical_header(&mut output);
            for line in &biblio_lines {
                let line_html = self
                    .render_line_with_context(line, &mut node_renderer, &mut block_manager)
                    .0;
                // 自動リンク化を適用
                let line_html = auto_link(&line_html);
                output.push_str(&line_html);
                output.push_str("<br />\r\n");
            }
            doc_renderer.render_bibliographical_footer(&mut output, input.ends_with('\n'));
        }

        // 表記について（notation_notes）セクション
        doc_renderer.render_notation_notes(
            &mut output,
            node_renderer.has_notes,
            node_renderer.has_jisx0213,
            node_renderer.has_accent,
            node_renderer.has_kunoji,
            node_renderer.has_dakuten_kunoji,
            &node_renderer.unconverted_gaiji,
        );

        // 図書カードセクション
        doc_renderer.render_card_section(&mut output);

        doc_renderer.render_html_foot(&mut output);

        output
    }

    /// 1行をHTMLに変換（コンテキスト付き）。
    /// 戻り値の bool は、その行が ［＃ここで…終わり］でブロックを閉じたか
    /// （参照実装の @terprip=false 相当。true なら行末の <br /> を出さない）。
    fn render_line_with_context(
        &self,
        line: &str,
        node_renderer: &mut NodeRenderer,
        block_manager: &mut BlockManager,
    ) -> (String, bool) {
        // くの字点は注記の中に書かれることもあるので生の行から数える
        node_renderer.scan_kunoji(line);

        let tokens = tokenize(line);
        let mut nodes = parse(&tokens);

        // 行内ルビを解決
        resolve_inline_ruby(&mut nodes);

        // ［＃ここで…終わり］形式でブロックを閉じた行かどうか。
        // 参照実装 exec_block_end_command はこの形式で @terprip=false を立て、
        // その行の行末 <br /> を抑制する（同一行で開閉した横組みや、複数行
        // ブロックの閉じ行が該当する）。bare ［＃…終わり］は抑制しない。
        let has_explicit_close = nodes
            .iter()
            .any(|n| matches!(n, Node::BlockEnd { params, .. } if params.explicit_close));

        // 行単位字下げ ［＃N字下げ］の扱い（参照実装 apply_jisage 相当）
        if let Some(pos) = nodes
            .iter()
            .position(|n| matches!(n, Node::LineJisage { .. }))
        {
            let Node::LineJisage { width } = nodes[pos] else {
                unreachable!()
            };
            // 行にこのコマンドしかなければ、その行から複数行ブロックを開く
            if nodes.len() == 1 {
                block_manager.push(
                    BlockType::Jisage,
                    BlockParams {
                        width: Some(width),
                        is_block: true,
                        ..Default::default()
                    },
                );
                return (
                    format!("<div class=\"jisage_{width}\" style=\"margin-left: {width}em\">"),
                    true,
                );
            }
            // テキストがあれば、コマンドを取り除いて行全体を字下げの div で包む
            nodes.remove(pos);
            let inner = node_renderer.render_nodes(&nodes, block_manager);
            return (
                format!(
                    "<div class=\"jisage_{width}\" style=\"margin-left: {width}em\">{inner}</div>"
                ),
                has_explicit_close,
            );
        }

        // 行の開始時点でのブロックスタックの長さを記録
        let stack_len_before = block_manager.stack_len();

        let mut output = node_renderer.render_nodes(&nodes, block_manager);

        // 行単位地付き: 行の終わりで、その行で開いたブロックを閉じる
        let is_line_scope_block = line.starts_with("［＃")
            && !line.contains("ここから")
            && (line.contains("地付き") || line.contains("地から"));

        if is_line_scope_block {
            let popped = block_manager.pop_to_length(stack_len_before);
            for (block_type, params) in popped {
                output.push_str(&block_manager.render_block_end_tag(&block_type, &params));
            }
        }

        (output, has_explicit_close)
    }

    /// 1行をHTMLに変換（公開API）
    pub fn render_line(&mut self, line: &str) -> String {
        let mut node_renderer = NodeRenderer::new(&self.options);
        let mut block_manager = BlockManager::new();
        self.render_line_with_context(line, &mut node_renderer, &mut block_manager)
            .0
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
