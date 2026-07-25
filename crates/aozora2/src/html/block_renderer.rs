//! 中立AST（[`Block`] 木）を本文HTMLに変換する新バックエンド。
//!
//! docs/plan-neutral-ast.md Phase B4。旧 `renderer.rs`＋`BlockManager` を置き換える
//! ことを目指すが、まだ**最小核**（jisage の Nested と内容行、インラインは Text
//! のみ）。旧経路と本文HTMLが byte 一致することを確認しながら記法を1種類ずつ足す。
//! バックエンドは木を**状態なしに歩く**だけ（BlockManager を持たない）。

use aozora_core::ast::{Block, BlockKind, Break, Inline};
use aozora_core::node::RubyDirection;

use super::presentation::{html_escape, style_css_class, style_html_tag};

/// ブロック列を本文HTML（main_text の内側）に変換する。
pub fn render_body_blocks(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        render_block(block, &mut out);
    }
    out
}

fn render_block(block: &Block, out: &mut String) {
    match block {
        Block::Line { inline, brk } => {
            render_inlines(inline, out);
            if *brk == Break::Br {
                out.push_str("<br />");
            }
            out.push_str("\r\n");
        }
        Block::Nested {
            kind,
            children,
            explicit_close,
        } => render_nested(kind, children, *explicit_close, out),
    }
}

fn render_nested(kind: &BlockKind, children: &[Block], explicit_close: bool, out: &mut String) {
    // 閉じタグ直後の改行は互換メタデータで決める（暗黙閉じは次の開きと同じ行）。
    let close_nl = if explicit_close { "</div>\r\n" } else { "</div>" };
    match kind {
        BlockKind::Jisage { width } => {
            out.push_str(&format!(
                "<div class=\"jisage_{width}\" style=\"margin-left: {width}em\">\r\n"
            ));
            for child in children {
                render_block(child, out);
            }
            out.push_str(close_nl);
        }
        // TODO: 他の BlockKind を段階的に足す。
        _ => {
            for child in children {
                render_block(child, out);
            }
        }
    }
}

fn render_inlines(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        render_inline(inline, out);
    }
}

fn render_inline(inline: &Inline, out: &mut String) {
    match inline {
        Inline::Text(s) => out.push_str(&html_escape(s)),
        Inline::Style {
            children,
            style_type,
        } => {
            let tag = style_html_tag(*style_type);
            let class = style_css_class(*style_type);
            out.push_str(&format!("<{tag} class=\"{class}\">"));
            render_inlines(children, out);
            out.push_str(&format!("</{tag}>"));
        }
        Inline::Tcy { children } => wrap_span(children, "dir=\"ltr\"", out),
        Inline::Keigakomi { children } => wrap_span(children, "class=\"keigakomi\"", out),
        Inline::Yokogumi { children } => wrap_span(children, "class=\"yokogumi\"", out),
        Inline::Caption { children } => wrap_span(children, "class=\"caption\"", out),
        Inline::Warigaki { children } => wrap_span(children, "class=\"warigaki\"", out),
        Inline::Ruby {
            base,
            ruby,
            direction,
            ..
        } => {
            // 外字を含まない親文字では notes 分割は起きない（両分岐とも同描画）。
            // 外字入り親文字（UnEmbedGaiji の rb 外出し）は Gaiji 実装時に足す。
            let mut base_html = String::new();
            render_inlines(base, &mut base_html);
            let mut ruby_html = String::new();
            render_inlines(ruby, &mut ruby_html);
            let ruby_html = ruby_html.replace('\u{00a0}', "&nbsp;");
            let open = match direction {
                RubyDirection::Right => "<ruby>",
                RubyDirection::Left => "<ruby class=\"leftrb\">",
            };
            out.push_str(&format!(
                "{open}<rb>{base_html}</rb><rp>（</rp><rt>{ruby_html}</rt><rp>）</rp></ruby>"
            ));
        }
        // TODO: Gaiji/Accent/Img/Note/FontSize/Midashi/Warichu/… を足す。
        _ => {}
    }
}

/// `<span {attr}>{children}</span>` で包む（縦中横・罫囲み・横組み・キャプション等）。
fn wrap_span(children: &[Inline], attr: &str, out: &mut String) {
    out.push_str(&format!("<span {attr}>"));
    render_inlines(children, out);
    out.push_str("</span>");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::{convert, RenderOptions};
    use aozora_core::document::extract_body_lines;
    use aozora_core::lower::lower_to_blocks;
    use aozora_core::parser::parse_document_raw;

    /// 旧経路（convert）と新経路（lower_to_blocks→render_body_blocks）の本文HTMLが
    /// jisage 文書で byte 一致することを固定する（垂直スライスの最小核）。
    /// インラインは Text のみなので本文はプレーンテキストに限る。
    fn assert_body_matches(src: &str) {
        let old = convert(src, &RenderOptions::default());
        let open = "<div class=\"main_text\">";
        let close = "</div>\r\n<div class=\"bibliographical_information\">";
        let start = old.find(open).expect("main_text open") + open.len();
        let end = old.find(close).expect("bibliographical close");
        let old_inner = &old[start..end];

        let mut lines: Vec<&str> = src.split("\r\n").collect();
        if lines.last() == Some(&"") {
            lines.pop();
        }
        let body_lines = extract_body_lines(&lines);
        let raw = parse_document_raw(&body_lines);
        let blocks = lower_to_blocks(&raw);
        let new_inner = render_body_blocks(&blocks);

        assert_eq!(new_inner, old_inner, "\n新:{new_inner:?}\n旧:{old_inner:?}");
    }

    #[test]
    fn test_jisage_body_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n前文\r\n［＃ここから２字下げ］\r\n内容A\r\n内容B\r\n［＃ここで字下げ終わり］\r\n後文\r\n底本：テスト\r\n",
        );
    }

    /// 連続 jisage は参照では兄弟（implicit_close）。ここから1字下げ→ここから3字下げは
    /// jisage_1 を閉じてから jisage_3 を開く。
    #[test]
    fn test_sibling_jisage_body_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n［＃ここから１字下げ］\r\nA\r\n［＃ここから３字下げ］\r\nB\r\n［＃ここで字下げ終わり］\r\n後\r\n底本：テスト\r\n",
        );
    }

    /// 装飾（傍点＝Style）・縦中横（Tcy）を含む内容行が旧経路と byte 一致すること。
    #[test]
    fn test_inline_style_tcy_body_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n対象［＃「対象」に傍点］の文と12［＃「12」は縦中横］日\r\n底本：テスト\r\n",
        );
    }

    /// ルビ（外字を含まない一般ケース）が旧経路と byte 一致すること。
    #[test]
    fn test_inline_ruby_body_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n東京《とうきょう》の｜山手線《やまのてせん》に乗る\r\n底本：テスト\r\n",
        );
    }

    /// jisage の中に装飾を含む内容行。
    #[test]
    fn test_jisage_with_inline_style_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n［＃ここから２字下げ］\r\n強い［＃「強い」は太字］語\r\n［＃ここで字下げ終わり］\r\n底本：テスト\r\n",
        );
    }

    /// 順に開閉する2つの jisage ブロック。
    #[test]
    fn test_sequential_jisage_body_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n［＃ここから２字下げ］\r\nA\r\n［＃ここで字下げ終わり］\r\n間\r\n［＃ここから４字下げ］\r\nB\r\n［＃ここで字下げ終わり］\r\n後\r\n底本：テスト\r\n",
        );
    }
}
