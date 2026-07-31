//! Lowerer のインライン層: RawAST のノード列 → Aozora AST の [`Inline`] 列。
//!
//! ブロック層（[`super::lower_to_blocks`]）が行を切り出したあと、その行のノード列を
//! ここでインラインの木に畳む。同一行に開閉が揃う範囲コマンド（見出し・装飾・
//! 大小文字・横組み等）と、行の途中で開くブロック形・地付きもここで畳む。
//!
//! ブロック構造マーカー（`BlockStart`/`BlockEnd` の is_block=true・`LineJisage`・
//! `UnresolvedReference`）はインラインにならず、ブロック層が消費する。

use crate::ast::{Inline, InlineKind};
use crate::node::{BlockParams, BlockType, Node, NodeKind};
use crate::parser::parse;
use crate::parser::reference_resolver::resolve_inline_ruby;
use crate::token::Span;
use crate::tokenizer::tokenize;

/// 注記の中身を再パースする深さの上限（注記が注記を含む入れ子の暴走防止）。
const MAX_NOTE_DEPTH: usize = 4;

/// RawAST の [`Node`] のインライン変種を Aozora AST の [`Inline`] に写す。
/// ブロック構造マーカーは None を返す（ブロック畳み込みが別途消費する）。
/// 割り注（apply_warichu）は状態を持たないインライン出力なので、`BlockStart`/
/// `BlockEnd` の Warichu だけは [`InlineKind::Warichu`] マーカーとして写す。
pub fn inline_from_node(node: &Node) -> Option<Inline> {
    inline_from_node_at(node, 0)
}

/// [`inline_from_node`] の本体。`depth` は注記の中身を再パースした深さ。
fn inline_from_node_at(node: &Node, depth: usize) -> Option<Inline> {
    let out = match &node.kind {
        NodeKind::Text(s) => InlineKind::Text(s.clone()),
        NodeKind::Ruby {
            children,
            ruby,
            direction,
            keep_gaiji_notes_in_base,
        } => InlineKind::Ruby {
            base: to_inlines_at(children, depth),
            ruby: to_inlines_at(ruby, depth),
            direction: *direction,
            keep_gaiji_notes_in_base: *keep_gaiji_notes_in_base,
        },
        NodeKind::Style {
            children,
            style_type,
        } => InlineKind::Style {
            children: to_inlines_at(children, depth),
            style_type: *style_type,
        },
        NodeKind::Midashi {
            children,
            level,
            style,
        } => InlineKind::Midashi {
            children: to_inlines_at(children, depth),
            level: *level,
            style: *style,
        },
        NodeKind::Gaiji {
            description,
            unicode,
            jis_code,
            had_igeta,
        } => InlineKind::Gaiji {
            description: description.clone(),
            unicode: unicode.clone(),
            jis_code: jis_code.clone(),
            had_igeta: *had_igeta,
        },
        NodeKind::Accent {
            code,
            name,
            unicode,
        } => InlineKind::Accent {
            code: code.clone(),
            name: name.clone(),
            unicode: unicode.clone(),
        },
        NodeKind::Img {
            filename,
            alt,
            is_photo,
            width,
            height,
        } => InlineKind::Img {
            filename: filename.clone(),
            alt: alt.clone(),
            is_photo: *is_photo,
            width: *width,
            height: *height,
        },
        NodeKind::Tcy { children } => InlineKind::Tcy {
            children: to_inlines_at(children, depth),
        },
        NodeKind::Keigakomi { children } => InlineKind::Keigakomi {
            children: to_inlines_at(children, depth),
        },
        NodeKind::Yokogumi { children } => InlineKind::Yokogumi {
            children: to_inlines_at(children, depth),
        },
        NodeKind::Caption { children } => InlineKind::Caption {
            children: to_inlines_at(children, depth),
        },
        NodeKind::FontSize {
            children,
            size_type,
            level,
        } => InlineKind::FontSize {
            children: to_inlines_at(children, depth),
            size_type: *size_type,
            level: *level,
        },
        NodeKind::Kaeriten(s) => InlineKind::Kaeriten(s.clone()),
        NodeKind::Okurigana(s) => InlineKind::Okurigana {
            content: note_content(s, depth),
            raw: s.clone(),
        },
        NodeKind::Note(s) => InlineKind::Note {
            content: note_content(s, depth),
            raw: s.clone(),
        },
        NodeKind::DakutenKatakana { num } => InlineKind::DakutenKatakana { num: num.clone() },
        NodeKind::AnnotationEnd {
            prefix,
            content,
            suffix,
        } => InlineKind::AnnotationEnd {
            prefix: prefix.clone(),
            content: to_inlines_at(content, depth),
            suffix: suffix.clone(),
        },
        // 割り注は apply_warichu の状態なし出力。開閉をマーカーとして写す。
        NodeKind::BlockStart {
            block_type: BlockType::Warichu,
            params,
        } => InlineKind::Warichu {
            open: true,
            suppress_paren: params.has_open_paren,
        },
        NodeKind::BlockEnd {
            block_type: BlockType::Warichu,
            params,
            ..
        } => InlineKind::Warichu {
            open: false,
            suppress_paren: params.has_close_paren,
        },
        // 未解決参照が Aozora AST に来ることはない。Lowerer は行ごとに
        // `resolve_references` を通し、解決できなかったものは `Note(raw)` にする
        // （docs/spec-ast.md「Aozora AST の特徴」不変条件1）。ここへ来たら
        // 解決を飛ばした呼び出しなので、黙って落とさず気付けるようにする。
        NodeKind::UnresolvedReference { raw, .. } => {
            debug_assert!(
                false,
                "未解決参照が Aozora AST に来た（resolve_references を通していない）: {raw}"
            );
            return None;
        }
        // ブロック構造マーカーはインラインではない（畳み込みが消費）。
        NodeKind::BlockStart { .. }
        | NodeKind::BlockEnd { .. }
        | NodeKind::LineJisage { .. } => return None,
    };
    Some(Inline::new(out, node.span))
}

/// 解決済みノード列を Aozora AST のインライン列に変換する。
///
/// 先頭から走査し、`BlockStart` で始まる範囲は [`fold_block_start`] が1つの
/// [`Inline`] に畳む（畳めなければ1ノードずつ写す）。畳めないブロックマーカーは
/// 除外する（ブロック層が消費するか、未対応）。
pub fn to_inlines(nodes: &[Node]) -> Vec<Inline> {
    to_inlines_at(nodes, 0)
}

/// [`to_inlines`] の本体。`depth` は注記の中身を再パースした深さ。
fn to_inlines_at(nodes: &[Node], depth: usize) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < nodes.len() {
        if let Some((inline, next)) = fold_block_start(nodes, i, depth) {
            out.push(inline);
            i = next;
            continue;
        }
        if let Some(inline) = inline_from_node_at(&nodes[i], depth) {
            out.push(inline);
        }
        i += 1;
    }
    out
}

/// `nodes[i]` から始まる範囲を1つの [`Inline`] に畳む。
/// 返り値は畳んだインラインと、走査を再開する添字。
fn fold_block_start(nodes: &[Node], i: usize, depth: usize) -> Option<(Inline, usize)> {
    let NodeKind::BlockStart { block_type, params } = &nodes[i].kind else {
        return None;
    };
    fold_inline_range(nodes, i, block_type, params, depth)
        .or_else(|| fold_block_inline(nodes, i, block_type, params, depth))
        .or_else(|| fold_trailing_chitsuki(nodes, i, block_type, params, depth))
}

/// 注記・送り仮名の中身（生の注記文字列）を解決済みインライン列に畳む。
///
/// 参照実装は注記の中身を別の TagParser に渡して描画するので、ここで同じ
/// tokenize→parse→ルビ解決を通す（前方参照 `resolve_references` は注記の中では
/// 走らせない＝参照実装と同じ）。深さ上限に達したら再パースせず素のテキストにする。
fn note_content(raw: &str, depth: usize) -> Vec<Inline> {
    let span = Span::new(0, raw.chars().count());
    if depth >= MAX_NOTE_DEPTH {
        return vec![Inline::text(raw, span)];
    }
    let mut nodes = parse(&tokenize(raw));
    resolve_inline_ruby(&mut nodes);
    to_inlines_at(&nodes, depth + 1)
}

/// 同一行に開閉が揃うインライン範囲コマンド `［＃中見出し］…［＃中見出し終わり］`
/// （`BlockStart{is_block=false}` … `BlockEnd`）をインライン要素に畳む
/// （参照実装は block stack への push/pop で同行に h4 を開閉する）。
fn fold_inline_range(
    nodes: &[Node],
    i: usize,
    block_type: &BlockType,
    params: &BlockParams,
    depth: usize,
) -> Option<(Inline, usize)> {
    if params.is_block {
        return None;
    }
    // 畳める種類かどうかは wrap_inline_range が Option で答える（前段で同じ
    // 一覧を持たない）。畳めない種類はここまで来ても最後に None になる。
    let end = find_matching_end(nodes, i, block_type)?;
    let inner = to_inlines_at(&nodes[i + 1..end], depth);
    let inline = wrap_inline_range(block_type, params, inner, span_for_nodes(&nodes[i..=end]))?;
    Some((inline, end + 1))
}

/// 行の途中で開閉する**ブロック形**（is_block=true、例:
/// `TEXT［＃ここから横組み］…［＃ここで横組み終わり］`）を [`InlineKind::BlockInline`]
/// に畳む。参照はブロック開始タグ（div/h4）を行内に埋め込む。
fn fold_block_inline(
    nodes: &[Node],
    i: usize,
    block_type: &BlockType,
    params: &BlockParams,
    depth: usize,
) -> Option<(Inline, usize)> {
    if !params.is_block {
        return None;
    }
    let end = find_matching_end(nodes, i, block_type)?;
    let kind = super::block_kind_of(block_type, params)?;
    let children = to_inlines_at(&nodes[i + 1..end], depth);
    let inline = Inline::from_range(
        InlineKind::BlockInline { kind, children },
        span_for_nodes(&nodes[i..=end]),
    );
    Some((inline, end + 1))
}

/// 行の途中で開く地付き（is_block=false の Chitsuki）を行末まで包む
/// （参照 close_inline_blocks が行末で閉じる）。行頭のものは classify_line が
/// LineWrap で処理済みなので、ここに来るのは本文の後に続くケース。
///
/// 範囲由来だが `range_form` は立てない。参照でこれは `Tag::OnelineIndent` で、
/// `blank_type` は String（→包む）と OnelineIndent（→`:inline`）を別扱いし、
/// **先に見つかった方**を返す。ここに来る＝先に本文があるので参照も String 側＝
/// 包む判定になり、`range_form` で中身を見る必要はない。
fn fold_trailing_chitsuki(
    nodes: &[Node],
    i: usize,
    block_type: &BlockType,
    params: &BlockParams,
    depth: usize,
) -> Option<(Inline, usize)> {
    if params.is_block || *block_type != BlockType::Chitsuki {
        return None;
    }
    // 閉じが無ければ行末までが中身（参照 close_inline_blocks）。
    let end = find_matching_end(nodes, i, block_type);
    let content_end = end.unwrap_or(nodes.len());
    let consumed_end = end.map_or(nodes.len(), |end| end + 1);
    let children = to_inlines_at(&nodes[i + 1..content_end], depth);
    let inline = Inline::new(
        InlineKind::ChitsukiInline {
            width: params.width.unwrap_or(0),
            children,
        },
        span_for_nodes(&nodes[i..consumed_end]),
    );
    Some((inline, consumed_end))
}

/// インライン範囲コマンド（見出し・装飾・大小文字・横組み・縦中横・罫囲み・
/// キャプション・割書）の開閉対を、対応する [`Inline`] に包む。畳めない種類は None。
///
/// 「どの種類が同行に畳めるか」の知識はこの網羅マッチだけが持つ。以前は
/// `is_inline_range_type` が同じ一覧を別に持ち、[`fold_inline_range`] の前段
/// ガードに使っていたが、片方にだけ種類を足すとガードは通ってここが None を返し、
/// その記法が**黙って注記化**する。ここが Option を返すので前段ガードは不要。
///
/// **`_` の catch-all を置かないこと。** 置くと [`BlockType`] に variant を
/// 足したとき同じずれが再発する。
fn wrap_inline_range(
    block_type: &BlockType,
    params: &BlockParams,
    inner: Vec<Inline>,
    span: Span,
) -> Option<Inline> {
    use crate::node::FontSizeType;
    let kind = match block_type {
        BlockType::Midashi => Some(InlineKind::Midashi {
            children: inner,
            level: params.level.unwrap_or(crate::node::MidashiLevel::O),
            style: params
                .midashi_style
                .unwrap_or(crate::node::MidashiStyle::Normal),
        }),
        BlockType::Style => params.style_type.map(|style_type| InlineKind::Style {
            children: inner,
            style_type,
        }),
        BlockType::FontDai => Some(InlineKind::FontSize {
            children: inner,
            size_type: FontSizeType::Dai,
            level: params.font_size.unwrap_or(1),
        }),
        BlockType::FontSho => Some(InlineKind::FontSize {
            children: inner,
            size_type: FontSizeType::Sho,
            level: params.font_size.unwrap_or(1),
        }),
        BlockType::Yokogumi => Some(InlineKind::Yokogumi { children: inner }),
        BlockType::Tcy => Some(InlineKind::Tcy { children: inner }),
        BlockType::Keigakomi => Some(InlineKind::Keigakomi { children: inner }),
        BlockType::Caption => Some(InlineKind::Caption { children: inner }),
        BlockType::Warigaki => Some(InlineKind::Warigaki { children: inner }),
        // 同行に畳まない種類。ブロック形（is_block=true）は
        // [`fold_block_inline`] が BlockInline として扱い、割り注・地付きは
        // それぞれ専用の経路を持つ。
        BlockType::Jisage
        | BlockType::Chitsuki
        | BlockType::Jizume
        | BlockType::Futoji
        | BlockType::Shatai
        | BlockType::Warichu
        | BlockType::Burasage
        | BlockType::AnnotationRange
        | BlockType::LeftAnnotationRange => None,
    };
    kind.map(|kind| Inline::from_range(kind, span))
}

/// `start` の `BlockStart` に対応する同種の `BlockEnd` の添字を返す（入れ子対応）。
/// `nodes[start]` は `block_type` の `BlockStart` であること（呼び出し側が保証する）。
fn find_matching_end(nodes: &[Node], start: usize, block_type: &BlockType) -> Option<usize> {
    debug_assert!(
        matches!(&nodes[start].kind, NodeKind::BlockStart { block_type: bt, .. } if bt == block_type),
        "find_matching_end は対応する BlockStart から呼ぶこと"
    );
    let mut depth = 0usize;
    for (offset, node) in nodes.iter().enumerate().skip(start) {
        match &node.kind {
            NodeKind::BlockStart { block_type: bt, .. } if bt == block_type => depth += 1,
            NodeKind::BlockEnd { block_type: bt, .. } if bt == block_type => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// ノード列全体を覆う span。空スライスでは呼ばない（畳み込む範囲は必ず1つ以上）。
fn span_for_nodes(nodes: &[Node]) -> Span {
    let mut spans = nodes.iter().map(|node| node.span);
    let first = spans
        .next()
        .expect("an inline conversion range is never empty");
    spans.fold(first, Span::union)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use crate::tokenizer::tokenize;

    /// 解決済みノード列 → Inline 列（純インライン内容）。ブロックマーカーを含まない
    /// 一般的な行はこれで写せることを固定する。
    #[test]
    fn test_to_inlines_pure_inline() {
        let nodes = parse(&tokenize("東京《とうきょう》の本文※［＃「丸印」、U+25CB］"));
        let inlines = to_inlines(&nodes);
        // ルビ・テキスト・外字がインラインとして写ること。
        assert!(inlines
            .iter()
            .any(|i| matches!(i.kind, InlineKind::Ruby { .. })));
        assert!(inlines
            .iter()
            .any(|i| matches!(i.kind, InlineKind::Text(_))));
        assert!(inlines
            .iter()
            .any(|i| matches!(i.kind, InlineKind::Gaiji { .. })));
    }

    /// 割り注（apply_warichu）はブロックマーカーだがInlineKind::Warichuに写す。
    #[test]
    fn test_to_inlines_warichu_marker() {
        let nodes = parse(&tokenize("本文［＃割り注］注記［＃割り注終わり］"));
        let inlines = to_inlines(&nodes);
        let opens: Vec<&Inline> = inlines
            .iter()
            .filter(|i| matches!(i.kind, InlineKind::Warichu { open: true, .. }))
            .collect();
        let closes: Vec<&Inline> = inlines
            .iter()
            .filter(|i| matches!(i.kind, InlineKind::Warichu { open: false, .. }))
            .collect();
        assert_eq!(
            opens.len(),
            1,
            "割り注開きが InlineKind::Warichu にならない: {inlines:?}"
        );
        assert_eq!(
            closes.len(),
            1,
            "割り注終わりが InlineKind::Warichu にならない: {inlines:?}"
        );
        assert_eq!(opens[0].span, nodes[1].span);
        assert_eq!(closes[0].span, nodes[3].span);
    }

    /// ブロック構造マーカー（ここから字下げ）はインラインに現れない。
    #[test]
    fn test_to_inlines_skips_block_markers() {
        let nodes = parse(&tokenize("［＃ここから２字下げ］"));
        let inlines = to_inlines(&nodes);
        assert!(
            inlines.is_empty(),
            "ブロックマーカーがインライン化された: {inlines:?}"
        );
    }

    #[test]
    fn inline_inherits_node_spans_recursively_and_ignores_them_for_equality() {
        let nodes = parse(&tokenize("東京《とう》"));
        let inlines = to_inlines(&nodes);
        let ruby_node = nodes
            .iter()
            .find(|node| matches!(node.kind, NodeKind::Ruby { .. }))
            .expect("ruby node");
        let ruby_inline = inlines
            .iter()
            .find(|inline| matches!(inline.kind, InlineKind::Ruby { .. }))
            .expect("ruby inline");

        assert_eq!(ruby_inline.span, ruby_node.span);
        let InlineKind::Ruby { base, ruby, .. } = &ruby_inline.kind else {
            unreachable!();
        };
        assert_eq!(base[0].span, Span::new(0, 2));
        assert_eq!(ruby[0].span, Span::new(3, 5));
        assert!(ruby_inline.span.contains(base[0].span));
        assert!(ruby_inline.span.contains(ruby[0].span));
        assert_eq!(
            Inline::text("同じ", Span::new(0, 2)),
            Inline::text("同じ", Span::new(10, 12))
        );
    }

    #[test]
    fn inline_ranges_include_their_consumed_markers() {
        let source = "［＃太字］本文［＃太字終わり］";
        let nodes = parse(&tokenize(source));
        let inlines = to_inlines(&nodes);
        let style = inlines
            .iter()
            .find(|inline| matches!(inline.kind, InlineKind::Style { .. }))
            .expect("inline style");
        assert_eq!(style.span, span_for_nodes(&nodes));
        let InlineKind::Style { children, .. } = &style.kind else {
            unreachable!();
        };
        assert!(style.span.contains(children[0].span));
    }

    #[test]
    fn block_inline_and_chitsuki_include_their_consumed_source_ranges() {
        let block_nodes = parse(&tokenize(
            "前［＃ここから横組み］中［＃ここで横組み終わり］後",
        ));
        let block_inlines = to_inlines(&block_nodes);
        let block_inline = block_inlines
            .iter()
            .find(|inline| matches!(inline.kind, InlineKind::BlockInline { .. }))
            .expect("block inline");
        assert_eq!(block_inline.span, span_for_nodes(&block_nodes[1..4]));
        let InlineKind::BlockInline { children, .. } = &block_inline.kind else {
            unreachable!();
        };
        assert!(block_inline.span.contains(children[0].span));

        let chitsuki_nodes = parse(&tokenize("前［＃地付き］末"));
        let chitsuki_inlines = to_inlines(&chitsuki_nodes);
        let chitsuki = chitsuki_inlines
            .iter()
            .find(|inline| matches!(inline.kind, InlineKind::ChitsukiInline { .. }))
            .expect("chitsuki inline");
        assert_eq!(chitsuki.span, span_for_nodes(&chitsuki_nodes[1..]));
        let InlineKind::ChitsukiInline { children, .. } = &chitsuki.kind else {
            unreachable!();
        };
        assert!(chitsuki.span.contains(children[0].span));
    }

    /// 閉じの無い行途中の地付きは行末までを1つの ChitsukiInline に畳む
    /// （参照 close_inline_blocks が行末で閉じる）。
    #[test]
    fn trailing_chitsuki_without_end_wraps_to_the_end_of_line() {
        let nodes = parse(&tokenize("前［＃地から２字上げ］末尾まで"));
        let inlines = to_inlines(&nodes);
        let chitsuki = inlines
            .iter()
            .find(|inline| matches!(inline.kind, InlineKind::ChitsukiInline { .. }))
            .expect("chitsuki inline");
        let InlineKind::ChitsukiInline { width, children } = &chitsuki.kind else {
            unreachable!();
        };
        assert_eq!(*width, 2);
        assert_eq!(children, &to_inlines(&nodes[2..]));
        assert_eq!(chitsuki.span, span_for_nodes(&nodes[1..]));
    }

    /// 注記の中身は Lower 時に解決される（参照実装は注記を別の TagParser に渡す）。
    /// バックエンドが再パースしないための不変条件。
    #[test]
    fn note_content_is_resolved_at_lower_time() {
        let nodes = parse(&tokenize("［＃現代語訳「松籟《しょうらい》を聞く」］"));
        let inlines = to_inlines(&nodes);
        let InlineKind::Note { content, raw } = &inlines[0].kind else {
            panic!("注記にならない: {inlines:?}");
        };
        assert_eq!(raw, "現代語訳「松籟《しょうらい》を聞く」");
        assert!(
            content
                .iter()
                .any(|inline| matches!(inline.kind, InlineKind::Ruby { .. })),
            "注記の中のルビが解決されていない: {content:?}"
        );
    }

    /// 入れ子の注記は上限の深さまで再パースし、その先は素のテキストで残す。
    /// 中身にルビを置いて、解決されたかどうかで境界を見る。
    #[test]
    fn nested_notes_stop_reparsing_at_the_depth_limit() {
        /// 最も内側の注記の中身を返す。
        fn innermost_note_content(source: &str) -> Vec<Inline> {
            let inlines = to_inlines(&parse(&tokenize(source)));
            let mut content = match &inlines[0].kind {
                InlineKind::Note { content, .. } => content.clone(),
                other => panic!("注記にならない: {other:?}"),
            };
            while let Some(nested) = content.iter().find_map(|inline| match &inline.kind {
                InlineKind::Note { content, .. } => Some(content.clone()),
                _ => None,
            }) {
                content = nested;
            }
            content
        }

        let has_ruby = |content: &[Inline]| {
            content
                .iter()
                .any(|i| matches!(i.kind, InlineKind::Ruby { .. }))
        };

        // 上限の深さ（MAX_NOTE_DEPTH=4）までは中身を解決する。
        let within = innermost_note_content(
            "［＃「A」は［＃「B」は［＃「C」は［＃「D」は松《まつ》］］］］",
        );
        assert!(has_ruby(&within), "上限内で解決されていない: {within:?}");

        // それより深い注記は再パースせず、素のテキストのまま残す。
        let beyond = innermost_note_content(
            "［＃「A」は［＃「B」は［＃「C」は［＃「D」は［＃「E」は松《まつ》］］］］］",
        );
        assert!(
            beyond
                .iter()
                .all(|inline| matches!(inline.kind, InlineKind::Text(_))),
            "深さ上限を超えて再パースしている: {beyond:?}"
        );
    }

    #[test]
    fn annotation_end_preserves_its_node_and_content_spans() {
        let node = Node::new(
            NodeKind::AnnotationEnd {
                prefix: "左に「".to_string(),
                content: vec![Node::text("注記", Span::new(6, 8))],
                suffix: "」の注記付き終わり".to_string(),
            },
            Span::new(4, 18),
        );
        let inline = inline_from_node(&node).expect("annotation end inline");
        assert_eq!(inline.span, node.span);
        let InlineKind::AnnotationEnd { content, .. } = &inline.kind else {
            unreachable!();
        };
        assert_eq!(content[0].span, Span::new(6, 8));
        assert!(inline.span.contains(content[0].span));
    }
}
