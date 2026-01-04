//! 行処理ロジック
//!
//! 1行のHTMLレンダリングとブロック制御を担当します。

use aozora_core::parser::parse;
use aozora_core::parser::resolve_inline_ruby;
use aozora_core::tokenizer::tokenize;

use super::block_manager::BlockManager;
use super::node_renderer::NodeRenderer;
use super::presentation::{classify_line, is_block_only_line, LineType};

/// 行処理結果
pub struct LineResult {
    /// レンダリングされたHTML
    pub html: String,
    /// 行タイプ
    pub line_type: LineType,
    /// ぶら下げコンテキスト（幅、テキストインデント）
    pub burasage_context: Option<(u32, i32)>,
}

/// 行処理ロジック
pub struct LineProcessor;

impl LineProcessor {
    /// 1行をHTMLに変換
    pub fn render_line(
        line: &str,
        node_renderer: &mut NodeRenderer,
        block_manager: &mut BlockManager,
    ) -> LineResult {
        let tokens = tokenize(line);
        let mut nodes = parse(&tokens);

        // 行内ルビを解決
        resolve_inline_ruby(&mut nodes);

        // 行の開始時点でのブロックスタックの長さを記録
        let stack_len_before = block_manager.stack_len();

        let mut html = node_renderer.render_nodes(&nodes, block_manager);

        // 行単位字下げ: 行の終わりで、その行で開いたブロックを閉じる
        let is_line_scope_block = line.starts_with("［＃")
            && !line.contains("ここから")
            && (line.contains("字下げ") || line.contains("地付き") || line.contains("地から"));

        if is_line_scope_block {
            let popped = block_manager.pop_to_length(stack_len_before);
            for (block_type, params) in popped {
                html.push_str(&block_manager.render_block_end_tag(&block_type, &params));
            }
        }

        // ぶら下げコンテキストを取得
        let burasage_context = block_manager.find_burasage_context();

        // 行タイプを判定
        let line_type = classify_line(&html);

        LineResult {
            html,
            line_type,
            burasage_context,
        }
    }

    /// ぶら下げブロック内の行をラップ
    pub fn wrap_burasage_line(line_html: &str, wrap_width: u32, text_indent: i32) -> String {
        format!(
            "<div class=\"burasage\" style=\"margin-left: {wrap_width}em; text-indent: {text_indent}em;\">{line_html}</div>\r\n"
        )
    }

    /// 行末の処理（<br />追加、インラインブロック閉じる）
    pub fn finalize_line(
        output: &mut String,
        line_html: &str,
        original_line: &str,
        block_manager: &mut BlockManager,
    ) {
        // line_htmlが空でかつ元の行も空じゃない場合（コマンドのみの行）は何も出力しない
        if line_html.is_empty() && !original_line.is_empty() {
            return;
        }

        output.push_str(line_html);

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
        } else if ends_with_div {
            // 現在の出力がdiv終了タグで終わる場合は<br />不要
            false
        } else {
            !is_block_only_line(line_html)
        };

        if needs_br {
            output.push_str("<br />");
        }
        output.push_str("\r\n");
    }

    /// すべての未閉じブロックを閉じる
    pub fn close_all_blocks(output: &mut String, block_manager: &mut BlockManager) {
        while let Some(ctx) = block_manager.pop() {
            output.push_str(&block_manager.render_block_end_tag(&ctx.block_type, &ctx.params));
        }
    }
}
