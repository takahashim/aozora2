//! プレーンテキスト変換（strip）
//!
//! 青空文庫形式のテキストからルビ・注記を除去してプレーンテキストに変換します。

use crate::document;
use crate::encoding;
use crate::gaiji::convert_gaiji;

/// 青空文庫形式のバイト列をプレーンテキストに変換（Aozora AST経由）。
///
/// エンコーディング自動判定（UTF-8 / Shift_JIS）、本文抽出（前付け・後付け除去）を
/// 行う。HTML バックエンド（`html::render_via_blocks`）と**同じ tokenize→parse→lower**
/// を共有し、終端の木歩きだけをプレーンテキスト用に差し替える（`CloseKind`/`Break`/
/// div/br 等の HTML 固有メタデータは一切見ない＝Aozora ASTがバックエンド非依存である
/// 実証）。ブロック開始/終了だけの行は木に畳まれて消えるので、余計な空行も出ない。
///
/// # Examples
///
/// ```
/// let input = "タイトル\n著者\n\n本文です\n底本：青空文庫";
/// let plain = aozora_core::strip::convert(input.as_bytes());
/// assert_eq!(plain, "本文です\n");
/// ```
pub fn convert(input: &[u8]) -> String {
    use crate::lower::lower_to_blocks;
    use crate::parser::parse_document_raw;

    let text = encoding::decode_to_utf8(input);
    let lines: Vec<&str> = text.lines().collect();
    let body_lines = document::extract_body_lines(&lines);
    let raw = parse_document_raw(&body_lines);
    let blocks = lower_to_blocks(&raw);

    let mut out_lines: Vec<String> = Vec::new();
    for b in &blocks {
        render_block_plain(b, &mut out_lines);
    }
    // 冒頭と末尾の空行を削除
    let start = out_lines.iter().position(|s| !s.is_empty()).unwrap_or(0);
    let end = out_lines
        .iter()
        .rposition(|s| !s.is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);
    if start >= end {
        String::new()
    } else {
        out_lines[start..end].join("\n") + "\n"
    }
}

/// ブロックを1行1文字列でプレーンテキスト化して push する。
fn render_block_plain(block: &crate::ast::Block, out: &mut Vec<String>) {
    use crate::ast::Block;
    match block {
        Block::Line { inline, .. } => {
            let mut s = String::new();
            render_inlines_plain(inline, &mut s);
            out.push(s);
        }
        Block::LineWrap { inline, .. } => {
            let mut s = String::new();
            render_inlines_plain(inline, &mut s);
            out.push(s);
        }
        Block::Nested { children, .. } => {
            for c in children {
                render_block_plain(c, out);
            }
        }
    }
}

/// インライン列をプレーンテキスト化する（読める文字だけ残す）。
fn render_inlines_plain(inlines: &[crate::ast::Inline], out: &mut String) {
    use crate::ast::InlineKind;
    for i in inlines {
        match &i.kind {
            InlineKind::Text(s) => out.push_str(s),
            // ルビは親文字だけ残す。
            InlineKind::Ruby { base, .. } => render_inlines_plain(base, out),
            // 装飾・範囲系は中身のテキストを残す。
            InlineKind::Style { children, .. }
            | InlineKind::Midashi { children, .. }
            | InlineKind::Tcy { children }
            | InlineKind::Keigakomi { children }
            | InlineKind::Yokogumi { children }
            | InlineKind::Caption { children }
            | InlineKind::Warigaki { children }
            | InlineKind::FontSize { children, .. }
            | InlineKind::ChitsukiInline { children, .. }
            | InlineKind::BlockInline { children, .. } => render_inlines_plain(children, out),
            InlineKind::AnnotationEnd { content, .. } => render_inlines_plain(content, out),
            // 外字は Unicode 文字列へ。解決済みの unicode / 句点コード（jis_code）を
            // 優先し（AST 解決後は description が空でも jis_code を持つ）、無ければ
            // description からのフォールバック（変換不能なら 〓）。
            InlineKind::Gaiji {
                description,
                unicode,
                jis_code,
                ..
            } => {
                if let Some(u) = unicode {
                    out.push_str(u);
                } else if let Some(u) = jis_code
                    .as_deref()
                    .and_then(crate::jis_table::jis_to_unicode)
                {
                    out.push_str(&u);
                } else {
                    out.push_str(&convert_gaiji(description));
                }
            }
            // アクセント分解は合成文字へ（無ければ空）。
            InlineKind::Accent { unicode, .. } => {
                if let Some(u) = unicode {
                    out.push_str(u);
                }
            }
            InlineKind::DakutenKatakana { num } => {
                out.push_str(crate::node::Node::dakuten_katakana_char(num))
            }
            // 注記・返り点・送り仮名・画像・割り注マーカーはコマンド由来なので落とす。
            InlineKind::Note(_)
            | InlineKind::Kaeriten(_)
            | InlineKind::Okurigana(_)
            | InlineKind::Img { .. }
            | InlineKind::Warichu { .. } => {}
        }
    }
}

/// 青空文庫形式の1行（またはインライン断片）をプレーンテキストに変換（Aozora AST経由）。
///
/// 前付け・後付けの除去を行わず、入力全体をインライン列として変換する。
///
/// # Examples
///
/// ```
/// let input = "吾輩《わがはい》は猫《ねこ》である";
/// let plain = aozora_core::strip::convert_line(input);
/// assert_eq!(plain, "吾輩は猫である");
/// ```
pub fn convert_line(input: &str) -> String {
    use crate::lower::inline::to_inlines;
    use crate::parser::parse;
    use crate::parser::reference_resolver::{resolve_inline_ruby, resolve_references};
    use crate::tokenizer::tokenize;

    let mut nodes = parse(&tokenize(input));
    resolve_references(&mut nodes);
    resolve_inline_ruby(&mut nodes);
    let inlines = to_inlines(&nodes);
    let mut out = String::new();
    render_inlines_plain(&inlines, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        assert_eq!(convert_line("こんにちは"), "こんにちは");
    }

    #[test]
    fn test_ruby_removed() {
        assert_eq!(convert_line("漢字《かんじ》"), "漢字");
    }

    #[test]
    fn test_prefixed_ruby() {
        assert_eq!(convert_line("｜東京《とうきょう》"), "東京");
    }

    #[test]
    fn test_command_removed() {
        assert_eq!(convert_line("猫である［＃「である」に傍点］"), "猫である");
    }

    #[test]
    fn test_gaiji_unicode() {
        assert_eq!(convert_line("※［＃「丸印」、U+25CB］"), "○");
    }

    #[test]
    fn test_complex() {
        assert_eq!(
            convert_line("吾輩《わがはい》は猫《ねこ》である［＃「である」に傍点］"),
            "吾輩は猫である"
        );
    }

    #[test]
    fn test_accent_conversion() {
        assert_eq!(convert_line("〔cafe'〕"), "café");
    }

    #[test]
    fn test_convert_with_header_footer() {
        let input = "タイトル\n著者\n\n本文です\n底本：青空文庫";
        let plain = convert(input.as_bytes());
        assert_eq!(plain, "本文です\n");
    }

    /// 第2バックエンド（Aozora AST経由）: ルビ除去・傍点の対象文字は残る。
    #[test]
    fn test_ast_basic() {
        let input = "T\n著\n\n吾輩《わがはい》は猫である［＃「である」に傍点］\n底本：青空文庫";
        assert_eq!(convert(input.as_bytes()), "吾輩は猫である\n");
    }

    /// 第2バックエンド: ブロック開始/終了だけの行は木に畳まれ、余計な空行が出ない。
    /// （旧トークン経路はコマンド行が空行として残る。）
    #[test]
    fn test_ast_drops_block_command_blank_lines() {
        let input =
            "T\n著\n\n本文1\n［＃ここから２字下げ］\n字下げ本文\n［＃ここで字下げ終わり］\n本文2\n底本：青空文庫";
        // Aozora AST版: ブロックマーカー行は消え、本文だけが連続する。
        assert_eq!(convert(input.as_bytes()), "本文1\n字下げ本文\n本文2\n");
    }
}
