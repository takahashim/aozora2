//! 中立AST（[`Block`] 木）を本文HTMLに変換する新バックエンド。
//!
//! docs/plan-neutral-ast.md Phase B4。旧 `renderer.rs`＋`BlockManager` を置き換える
//! ことを目指すが、まだ**最小核**（jisage の Nested と内容行、インラインは Text
//! のみ）。旧経路と本文HTMLが byte 一致することを確認しながら記法を1種類ずつ足す。
//! バックエンドは木を**状態なしに歩く**だけ（BlockManager を持たない）。

use aozora_core::ast::{Block, BlockKind, Break, Inline};

use super::presentation::html_escape;

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
        // TODO: 他の Inline を段階的に足す（Ruby/Gaiji/Style/…）。
        _ => {}
    }
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

    /// 順に開閉する2つの jisage ブロック。
    #[test]
    fn test_sequential_jisage_body_matches_old() {
        assert_body_matches(
            "題\r\n著\r\n\r\n［＃ここから２字下げ］\r\nA\r\n［＃ここで字下げ終わり］\r\n間\r\n［＃ここから４字下げ］\r\nB\r\n［＃ここで字下げ終わり］\r\n後\r\n底本：テスト\r\n",
        );
    }
}
