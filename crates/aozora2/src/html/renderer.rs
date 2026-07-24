//! HTMLレンダラー
//!
//! ASTノードをHTMLに変換します。

use aozora_core::document::{
    extract_after_text_lines, extract_bibliographical_lines, extract_body_lines,
    extract_header_info,
};
use aozora_core::node::{BlockParams, BlockType, Node};
use aozora_core::parser::reference_resolver::{resolve_inline_ruby, resolve_references};
use aozora_core::parser::{parse_document_raw, RawLine};

use super::block_manager::BlockManager;
use super::document_renderer::DocumentRenderer;
use super::node_renderer::NodeRenderer;
use super::options::RenderOptions;
use super::presentation::{auto_link, classify_output_line};

/// HTMLレンダラー
#[derive(Debug, Clone)]
pub struct HtmlRenderer {
    options: RenderOptions,
}

/// 行の（解決済み）ノード列がインラインの本文テキストを持つか。
/// 参照実装 TextBuffer#blank_type が false になる条件＝空でない String が
/// バッファに入っているかに対応する。ブロック制御ノード（ブロックの開閉・
/// 見出し・行単位字下げ）はテキストを持たず、それ以外のインライン要素
/// （テキスト・ルビ・外字・傍点・縦中横・注記・行中の地付き周辺のテキスト等）は
/// 本文テキストを生む。ぶら下げブロック内でこの行を div で包むかの判定に使う。
fn nodes_have_inline_text(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Text(s) => !s.is_empty(),
        // ブロック制御ノードと注記はテキスト（空でない String）を持たない扱い。
        // 参照実装では ［＃改ページ］等の注記だけの行は blank_type が true になり
        // ぶら下げで包まれず <br /> になる。
        // 画像（Tag::Img）は String ではなく Tag::Inline なので blank_type は
        // true のまま＝ぶら下げで包まれず、terpri? が true なので <br /> が付く。
        // ルビ（Tag::Ruby）も同様: 親文字は Ruby タグに取り込まれてバッファに
        // String を残さないので、行全体がルビだけなら blank_type は true になり
        // 包まれない。親文字の外に本文テキストがあればその Text ノードが true を
        // 返すのでその行は包まれる。
        // 装飾（傍点・傍線・太字・斜体など Node::Style）とインラインの文字サイズ
        // （Node::FontSize、「N段階大きな文字」等）も、参照実装では対象テキストを
        // その装飾タグ（<em>/<span>）に取り込んでバッファに String を残さない。
        // よって行全体がひとつの装飾／文字サイズだけなら blank_type は true で
        // 包まれず <br /> になる。対象の外に平文があればその Text ノードが true を
        // 返すのでその行は包まれる（例:「半分［＃「太字」は太字］のこり」）。
        Node::BlockStart { .. }
        | Node::BlockEnd { .. }
        | Node::Midashi { .. }
        | Node::LineJisage { .. }
        | Node::Img { .. }
        | Node::Ruby { .. }
        | Node::Style { .. }
        | Node::FontSize { .. }
        // 前方参照の縦中横（「32」は縦中横）は対象を Node::Tcy に取り込むので
        // 行全体がそれだけなら String を残さず包まれない。明示形
        // ［＃縦中横］32［＃縦中横終わり］は BlockStart/BlockEnd＋Text になり、
        // Text が残るので従来どおり包まれる（Node::Tcy にはならない）。
        | Node::Tcy { .. }
        | Node::Note(_) => false,
        _ => true,
    })
}

/// その行が「装飾系ブロックの閉じだけ」の行か。
/// 参照実装は、ぶら下げの中で入れ子に開いた装飾系ブロック（横組み・罫囲み・
/// キャプション・大小の文字・太字・斜体・字詰め）が閉じる行を、閉じタグを
/// String 扱いして空の `<div class="burasage"></div>` で包む。一方、字下げ・
/// 地付き（＝ぶら下げと同じインデント系）の閉じは包まない。ぶら下げを開いた行
/// （close_related_blocks で兄弟の字下げが閉じて `</div>` が出る場合を含む）は
/// BlockStart を含むのでここには該当しない。
fn is_decoration_block_close(nodes: &[Node]) -> bool {
    matches!(
        nodes,
        [Node::BlockEnd { block_type, .. }]
            if matches!(
                block_type,
                BlockType::Yokogumi
                    | BlockType::Keigakomi
                    | BlockType::Caption
                    | BlockType::FontDai
                    | BlockType::FontSho
                    | BlockType::Futoji
                    | BlockType::Shatai
                    | BlockType::Jizume
            )
    )
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

        // 本文を抽出し、文書単位で RawAST に実体化してからレンダリング
        let body_lines = extract_body_lines(&lines);
        let body_raw = parse_document_raw(&body_lines);
        for raw_line in &body_raw.lines {
            let line = raw_line.source.as_str();
            let (mut line_html, has_explicit_close, has_inline_text) =
                self.render_raw_line(raw_line, &mut node_renderer, &mut block_manager);

            // インラインブロック（is_block=false、例: 行中の ［＃地付き］）は行末で
            // 閉じ、line_html に取り込む。ぶら下げ行を div で包むときも閉じタグ込みで
            // 包めるよう、包む判定より前に閉じておく。
            let closed_blocks = block_manager.close_inline_blocks();
            for (block_type, params) in closed_blocks {
                line_html.push_str(&block_manager.render_block_end_tag(&block_type, &params));
            }

            // レンダリング済み行を一度だけ分類する（型付き信号）
            let info = classify_output_line(&line_html);

            // ぶら下げブロック内かどうかをチェック
            let burasage_ctx = block_manager.find_burasage_context();

            if let Some((wrap_width, text_indent)) = burasage_ctx {
                // ぶら下げブロック内: 本文テキストを持つ行を個別の div で包む。
                // 参照実装 general_output は blank_type==false（＝空でない String が
                // ある）の行だけ行頭で burasage div を開く。出力HTMLの末尾で
                // 判定すると `テキスト［＃地付き］右` のような行中インラインブロックを
                // 取りこぼすので、AST 由来の has_inline_text を使う。
                // ただし行全体が block div で始まる場合（行末の地付き/地からで
                // 行全体が chitsuki/jisage div になったとき）は、その行自体が
                // ブロック（Multiline）なので包まない＝先頭が <div。
                // 行頭がインラインの同行中見出し（<h4 class="dogyo-naka-midashi">）で
                // その後に本文テキストが続く行は blank_type が false（String あり）
                // なので参照実装は burasage で包む。よって <h は除外基準にしない。
                let starts_with_block = line_html.starts_with("<div");
                if has_inline_text && !starts_with_block {
                    // 折り返し幅が None（コンマなし）のとき、参照実装は margin-left を空に
                    // して不正な CSS `margin-left: em` を出す（Quirk empty_indent_css）。
                    // clean（オフ）なら妥当な 0em。
                    let margin = wrap_width.map(|w| w.to_string()).unwrap_or_else(|| {
                        if self.options.quirks.empty_indent_css {
                            String::new()
                        } else {
                            "0".to_string()
                        }
                    });
                    output.push_str(&format!(
                        "<div class=\"burasage\" style=\"margin-left: {margin}em; text-indent: {text_indent}em;\">{line_html}</div>"
                    ));
                    output.push_str("\r\n");
                    continue;
                }
                // ぶら下げ内の見出し行: 参照実装は per-line の burasage div を
                // 開かないが行末では閉じる（見出しが :midashi を indent_stack に
                // 積み、ぶら下げ div の収支がずれるため）。見出しタグ＋</div> を
                // 出し、<br /> は付けない。
                if info.is_midashi {
                    output.push_str(&line_html);
                    output.push_str("</div>\r\n");
                    continue;
                }
                // ぶら下げ内で入れ子に開いた装飾系ブロックが閉じる行。参照実装は
                // 閉じタグを String 扱いして空の burasage div で包む
                // （<div class="burasage">{閉じタグ}</div>）。字下げ・地付きの閉じや
                // ぶら下げ自身の開閉行はここに該当しない。
                if is_decoration_block_close(&raw_line.nodes) {
                    // 折り返し幅が None（コンマなし）のとき、参照実装は margin-left を空に
                    // して不正な CSS `margin-left: em` を出す（Quirk）。clean なら 0em。
                    let margin = wrap_width.map(|w| w.to_string()).unwrap_or_else(|| {
                        if self.options.quirks.empty_indent_css {
                            String::new()
                        } else {
                            "0".to_string()
                        }
                    });
                    output.push_str(&format!(
                        "<div class=\"burasage\" style=\"margin-left: {margin}em; text-indent: {text_indent}em;\">{line_html}</div>"
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
                !info.suppresses_br
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
            for raw_line in &parse_document_raw(&after_text_lines).lines {
                let line_html = self
                    .render_raw_line(raw_line, &mut node_renderer, &mut block_manager)
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
            for raw_line in &parse_document_raw(&biblio_lines).lines {
                let line_html = self
                    .render_raw_line(raw_line, &mut node_renderer, &mut block_manager)
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

    /// RawAST の1行をHTMLに変換（コンテキスト付き）。
    /// 戻り値の 2 番目 bool は、その行が ［＃ここで…終わり］でブロックを閉じたか
    /// （参照実装の @terprip=false 相当。true なら行末の <br /> を出さない）。
    /// 3 番目 bool は、その行がインラインの本文テキストを持つか
    /// （参照実装 TextBuffer#blank_type が false ＝行頭で ぶら下げ div を開く条件）。
    fn render_raw_line(
        &self,
        raw: &RawLine,
        node_renderer: &mut NodeRenderer,
        block_manager: &mut BlockManager,
    ) -> (String, bool, bool) {
        let line = raw.source.as_str();

        // くの字点は注記の中に書かれることもあるので生の行から数える
        node_renderer.scan_kunoji(line);

        // RawAST の生ノードを lower（前方参照を解決）してから描画する
        let mut nodes = raw.nodes.clone();
        resolve_references(&mut nodes);

        // 行内ルビを解決
        resolve_inline_ruby(&mut nodes);

        // ［＃ここで…終わり］形式でブロックを閉じた行かどうか。
        // 参照実装 exec_block_end_command はこの形式で @terprip=false を立て、
        // その行の行末 <br /> を抑制する（同一行で開閉した横組みや、複数行
        // ブロックの閉じ行が該当する）。bare ［＃…終わり］は抑制しない。
        let has_explicit_close = nodes.iter().any(|n| {
            matches!(
                n,
                Node::BlockEnd {
                    explicit_close: true,
                    ..
                }
            )
        });

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
                    false,
                );
            }
            // テキストがあれば、コマンドを取り除いて行全体を字下げの div で包む。
            // 行全体が字下げの block div になるので、ぶら下げ内でも本文テキスト行
            // としては包まない（参照実装でも jisage 行は burasage で包まれない）。
            nodes.remove(pos);
            let inner = node_renderer.render_nodes(&nodes, block_manager);
            return (
                format!(
                    "<div class=\"jisage_{width}\" style=\"margin-left: {width}em\">{inner}</div>"
                ),
                has_explicit_close,
                false,
            );
        }

        // 行の開始時点でのブロックスタックの長さを記録
        let stack_len_before = block_manager.stack_len();

        let has_inline_text = nodes_have_inline_text(&nodes);
        let mut output = node_renderer.render_nodes(&nodes, block_manager);

        // 行単位地付き/地から（LineChitsuki）: 行頭のコマンドでその行だけの
        // ブロックを開き、行末で閉じる。パーサが行スコープ（is_block=false）と
        // ブロックスコープ（［＃ここから…］= is_block=true）を型で区別済みなので、
        // 生文字列を詮索せず先頭ノードで判定する（本文中の「地付き」に誤反応しない）。
        let is_line_scope_block = matches!(
            nodes.first(),
            Some(Node::BlockStart {
                block_type: BlockType::Chitsuki,
                params,
            }) if !params.is_block
        );

        if is_line_scope_block {
            let popped = block_manager.pop_to_length(stack_len_before);
            for (block_type, params) in popped {
                output.push_str(&block_manager.render_block_end_tag(&block_type, &params));
            }
        }

        (output, has_explicit_close, has_inline_text)
    }

    /// 1行をHTMLに変換（公開API）
    pub fn render_line(&mut self, line: &str) -> String {
        let mut node_renderer = NodeRenderer::new(&self.options);
        let mut block_manager = BlockManager::new();
        let raw = parse_document_raw(&[line]);
        self.render_raw_line(&raw.lines[0], &mut node_renderer, &mut block_manager)
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
    fn test_midashi_inside_burasage_closes_an_extra_div() {
        // ぶら下げ（折り返し字下げ）ブロック内の見出し行は、参照実装では
        // per-line の burasage div を開かないまま行末で </div> を閉じる。
        // 前後の本文行は通常どおり burasage div で包まれる。
        let input = "題\r\n著\r\n\r\n\
            ［＃ここから２字下げ、折り返して３字下げ］\r\n\
            本文\r\n\
            【見出】［＃「【見出】」は中見出し］\r\n\
            次\r\n\
            ［＃ここで字下げ終わり］\r\n\r\n底本：「甲」乙\r\n";
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render(input);
        assert!(
            html.contains("</a></h4></div>\r\n"),
            "見出し行が行末で </div> を閉じていない: {html}"
        );
        assert!(
            html.contains(
                "<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -1em;\">次</div>"
            ),
            "見出し行の次の本文が burasage で包まれていない: {html}"
        );
    }

    #[test]
    fn test_burasage_wraps_line_with_inline_chitsuki() {
        // ぶら下げ内で、行の途中に ［＃地付き］ があってもテキスト行として
        // burasage div に包む（行末が </div> でも取りこぼさない）。
        let input = "題\r\n著\r\n\r\n\
            ［＃ここから２字下げ、折り返して３字下げ］\r\n\
            テキスト［＃地付き］右\r\n\
            次\r\n\
            ［＃ここで字下げ終わり］\r\n\r\n底本：「甲」乙\r\n";
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render(input);
        assert!(
            html.contains("<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -1em;\">テキスト<div class=\"chitsuki_0\" style=\"text-align:right; margin-right: 0em\">右</div></div>"),
            "行中地付きの行が burasage で包まれていない: {html}"
        );
        assert!(
            html.contains("<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -1em;\">次</div>"),
            "後続行の burasage 包みが失われている: {html}"
        );
    }

    #[test]
    fn test_burasage_wraps_nested_decoration_close() {
        // ぶら下げ内に入れ子で開いた装飾ブロック（小さな文字）が閉じる行は、
        // 参照実装では空の burasage div で包まれる（<div class="burasage"></div>）。
        let input = "題\r\n著\r\n\r\n\
            ［＃ここから２字下げ、折り返して３字下げ］\r\n\
            前の行\r\n\
            ［＃ここから１段階小さな文字］\r\n\
            小さい本文\r\n\
            ［＃ここで小さな文字終わり］\r\n\
            後の行\r\n\
            ［＃ここで字下げ終わり］\r\n\r\n底本：「甲」乙\r\n";
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render(input);
        assert!(
            html.contains("小さい本文<br />\r\n<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -1em;\"></div></div>"),
            "入れ子装飾ブロックの閉じ行が空 burasage div で包まれていない: {html}"
        );
    }

    #[test]
    fn test_burasage_does_not_wrap_inside_nested_block() {
        // ぶら下げの上に横組み等のブロックが開いている間は、ぶら下げは
        // 行を包まない（参照実装は @indent_stack.last が String のときだけ包む）。
        let input = "題\r\n著\r\n\r\n\
            ［＃ここから２字下げ、折り返して３字下げ］\r\n\
            前\r\n\
            ［＃ここから横組み］\r\n\
            横内\r\n\
            ［＃ここで横組み終わり］\r\n\
            後\r\n\
            ［＃ここで字下げ終わり］\r\n\r\n底本：「甲」乙\r\n";
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render(input);
        // 横組み内の行は burasage で包まれず、通常の <br /> になる
        assert!(html.contains("横内<br />"), "横組み内が包まれてしまった: {html}");
        // 横組みの外の行は包まれる
        assert!(
            html.contains("<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -1em;\">前</div>"),
            "横組み前の行が包まれていない: {html}"
        );
        assert!(
            html.contains("<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -1em;\">後</div>"),
            "横組み後の行が包まれていない: {html}"
        );
    }

    #[test]
    fn test_burasage_nocomma_has_empty_margin() {
        // コンマなし「折り返してN字下げ」は参照実装で margin-left 空・text-indent 0。
        // Quirk empty_indent_css オン（既定）＝参照一致（不正な margin-left: em）。
        let input = "題\r\n著\r\n\r\n\
            ［＃ここから折り返して３字下げ］\r\n\
            テキスト\r\n\
            ［＃ここで字下げ終わり］\r\n\r\n底本：「甲」乙\r\n";
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render(input);
        assert!(
            html.contains("<div class=\"burasage\" style=\"margin-left: em; text-indent: 0em;\">テキスト</div>"),
            "コンマなしぶら下げの margin-left が空になっていない: {html}"
        );
        // Quirk オフ＝妥当な CSS（margin-left: 0em）。
        let opts = RenderOptions {
            quirks: crate::html::options::Quirks::none(),
            ..RenderOptions::default()
        };
        let mut renderer = HtmlRenderer::new(opts);
        let html = renderer.render(input);
        assert!(
            html.contains("<div class=\"burasage\" style=\"margin-left: 0em; text-indent: 0em;\">テキスト</div>"),
            "Quirk オフでコンマなしぶら下げが margin-left: 0em になっていない: {html}"
        );
    }

    #[test]
    fn test_jisage_empty_width_quirk_paired() {
        // 全角空白で離れた「３　字下げ」は参照実装で空幅（jisage_ / margin-left: em）。
        // Quirk empty_indent_css オン（既定）＝参照一致（不正な CSS）。
        let input = "題\r\n著\r\n\r\n\
            ［＃ここから３　字下げ］\r\n\
            本文\r\n\
            ［＃ここで字下げ終わり］\r\n\r\n底本：「甲」乙\r\n";
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render(input);
        assert!(
            html.contains("<div class=\"jisage_\" style=\"margin-left: em\">"),
            "空幅字下げが jisage_ / margin-left: em になっていない: {html}"
        );
        // Quirk オフ＝妥当な `class=\"jisage\"`（不正な CSS を出さない）。
        let opts = RenderOptions {
            quirks: crate::html::options::Quirks::none(),
            ..RenderOptions::default()
        };
        let mut renderer = HtmlRenderer::new(opts);
        let html = renderer.render(input);
        assert!(
            html.contains("<div class=\"jisage\">") && !html.contains("margin-left: em"),
            "Quirk オフで空幅字下げが妥当な jisage になっていない: {html}"
        );
    }

    #[test]
    fn test_burasage_does_not_wrap_image_line() {
        // 画像だけの行は参照実装で blank_type が true（Tag::Img は String でない）
        // ＝ぶら下げで包まれず、terpri? が true なので <br /> が付く。
        let input = "題\r\n著\r\n\r\n\
            ［＃ここから１字下げ、折り返して３字下げ］\r\n\
            本文Ａ\r\n\
            ［＃図（fig01_01.png）入る］\r\n\
            本文Ｂ\r\n\
            ［＃ここで字下げ終わり］\r\n\r\n底本：「甲」乙\r\n";
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render(input);
        assert!(
            html.contains(
                "<img class=\"illustration\" width=\"\" height=\"\" src=\"fig01_01.png\" alt=\"図\" /><br />"
            ),
            "画像行がぶら下げで包まれず <br /> 付きになっていない: {html}"
        );
        assert!(
            !html.contains("<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -2em;\"><img"),
            "画像行がぶら下げで包まれてしまっている: {html}"
        );
        // 前後の本文行は従来どおり包まれる。
        assert!(html.contains("<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -2em;\">本文Ａ</div>"));
        assert!(html.contains("<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -2em;\">本文Ｂ</div>"));
    }

    #[test]
    fn test_burasage_wraps_inline_dogyo_midashi_with_trailing_text() {
        // 行頭がインラインの同行中見出し（<h4 class="dogyo-naka-midashi">）で
        // その後に本文テキストが続く行は、参照実装では blank_type が false
        // （末尾の本文 String がある）なので burasage で包む。
        let input = "題\r\n著\r\n\r\n\
            ［＃ここから１字下げ、折り返して３字下げ］\r\n\
            日付［＃「日付」は同行中見出し］　本文が続く。\r\n\
            ［＃ここで字下げ終わり］\r\n\r\n底本：「甲」乙\r\n";
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render(input);
        assert!(
            html.contains("<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -2em;\"><h4 class=\"dogyo-naka-midashi\">"),
            "同行中見出し＋本文の行が burasage で包まれていない: {html}"
        );
        assert!(
            html.contains("本文が続く。</div>"),
            "末尾の本文まで含めて burasage で閉じられていない: {html}"
        );
    }

    #[test]
    fn test_burasage_does_not_wrap_pure_ruby_line() {
        // 行全体がルビだけの行は、参照実装で親文字が Ruby タグに取り込まれ
        // バッファに String を残さない＝blank_type true なので包まれず <br /> になる。
        // 親文字の外に本文テキストがある行は従来どおり包まれる。
        let input = "題\r\n著\r\n\r\n\
            ［＃ここから１字下げ、折り返して３字下げ］\r\n\
            夢遊病者《ソムナンビユール》\r\n\
            前置き夢遊病者《ソムナンビユール》後置き\r\n\
            ［＃ここで字下げ終わり］\r\n\r\n底本：「甲」乙\r\n";
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render(input);
        assert!(
            html.contains("<ruby><rb>夢遊病者</rb><rp>（</rp><rt>ソムナンビユール</rt><rp>）</rp></ruby><br />"),
            "ルビだけの行がぶら下げで包まれず <br /> になっていない: {html}"
        );
        assert!(
            !html.contains("text-indent: -2em;\"><ruby>"),
            "ルビだけの行がぶら下げで包まれてしまっている: {html}"
        );
        // 親文字の外に本文がある行は包まれる。
        assert!(
            html.contains("<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -2em;\">前置き<ruby>"),
            "本文＋ルビの行が包まれていない: {html}"
        );
    }

    #[test]
    fn test_burasage_does_not_wrap_full_line_style_or_font() {
        // 行全体がひとつの装飾（太字）や文字サイズ（大きな文字）だけの行は、
        // 参照実装で対象テキストがタグに取り込まれバッファに String を残さない＝
        // blank_type true なので包まれず <br /> になる。対象の外に平文があれば包む。
        let input = "題\r\n著\r\n\r\n\
            ［＃ここから１字下げ、折り返して３字下げ］\r\n\
            全体大文字［＃「全体大文字」は２段階大きな文字］\r\n\
            全体太字［＃「全体太字」は太字］\r\n\
            半分太字［＃「太字」は太字］のこり\r\n\
            ［＃ここで字下げ終わり］\r\n\r\n底本：「甲」乙\r\n";
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render(input);
        assert!(
            html.contains("<span class=\"dai2\" style=\"font-size: x-large;\">全体大文字</span><br />"),
            "行全体が大きな文字の行が包まれず <br /> になっていない: {html}"
        );
        assert!(
            html.contains("<span class=\"futoji\">全体太字</span><br />"),
            "行全体が太字の行が包まれず <br /> になっていない: {html}"
        );
        assert!(
            !html.contains("text-indent: -2em;\"><span class=\"dai2\"")
                && !html.contains("text-indent: -2em;\"><span class=\"futoji\">全体"),
            "行全体装飾の行がぶら下げで包まれてしまっている: {html}"
        );
        // 対象の外に平文がある行は包まれる。
        assert!(
            html.contains("<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -2em;\">半分<span class=\"futoji\">太字</span>のこり</div>")
                || html.contains("<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -2em;\">半分太字<span class=\"futoji\">太字</span>のこり</div>"),
            "平文＋装飾の行が包まれていない: {html}"
        );
    }

    #[test]
    fn test_burasage_tcy_frontref_vs_explicit() {
        // 前方参照の縦中横（「32」は縦中横）は対象をタグに取り込むので、行全体が
        // それだけなら包まれず <br />。明示形 ［＃縦中横］32［＃縦中横終わり］は
        // 32 が String として残るので従来どおり包まれる。
        let input = "題\r\n著\r\n\r\n\
            ［＃ここから１字下げ、折り返して３字下げ］\r\n\
            32［＃「32」は縦中横］\r\n\
            ［＃縦中横］32［＃縦中横終わり］\r\n\
            ［＃ここで字下げ終わり］\r\n\r\n底本：「甲」乙\r\n";
        let mut renderer = HtmlRenderer::new(RenderOptions::default());
        let html = renderer.render(input);
        // 前方参照形: 包まれず <br />
        assert!(
            html.contains("<span dir=\"ltr\">32</span><br />"),
            "前方参照の縦中横だけの行が包まれず <br /> になっていない: {html}"
        );
        // 明示形: burasage で包まれる
        assert!(
            html.contains("<div class=\"burasage\" style=\"margin-left: 3em; text-indent: -2em;\"><span dir=\"ltr\">32</span></div>"),
            "明示形の縦中横の行が包まれていない: {html}"
        );
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
