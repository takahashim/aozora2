//! Lowerer: RawAST（平坦マーカー）→ Aozora AST（block ⊃ line ⊃ inline の木）
//!
//! architecture.md §4.1/§4.3・docs/plan-neutral-ast.md（Phase B2〜）。
//! 参照実装 `@indent_stack`/`implicit_close`/`@terprip` の逐次モデルを Lower 時に
//! 一度だけ計算し、ブロックを部分木に畳み、行末の改行を [`Break`] メタデータへ
//! 載せる。バックエンドはこの木を状態なしに歩くだけになる。
//!
//! **段階実装中**: まず jisage と内容行だけを畳む（垂直スライスの最小核）。
//! 旧 BlockManager 経路と本文HTMLが byte 一致することを確認しながら記法を1種類ずつ
//! 増やす。未対応のブロック種は暫定でトップレベルに落とす（TODO）。

use crate::ast::{AozoraAst, Block, BlockKind, Break, CloseKind, OpenKind};
use crate::node::{BlockType, Node, NodeKind};
use crate::parser::reference_resolver::{resolve_inline_ruby, resolve_references};
use crate::parser::RawDoc;

/// Lower 時に検出できる構造上の診断（現状は EOF で閉じられなかったブロック）。
/// エディタ支援用の付加情報で、変換出力には影響しない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerDiagnostic {
    /// ブロックを開いた本文行（0 起点）。
    pub line: usize,
    /// 閉じられなかったブロックの種類。
    pub kind: BlockKind,
}

/// RawDoc（未解決・平坦マーカー）を Aozora AST（[`AozoraAst`]＝トップレベル [`Block`] 列）に畳む。
pub fn lower_to_blocks(raw: &RawDoc) -> AozoraAst {
    lower_to_blocks_with_diagnostics(raw).0
}

/// [`lower_to_blocks`] と同じ畳み込みを行い、加えて構造上の診断（EOF で閉じられなかった
/// ブロック）を返す。**Block 出力は `lower_to_blocks` と完全一致**（診断は追加返却のみで
/// 変換結果には一切影響しない＝オラクル不変）。エディタ支援 `analysis` が使う。
pub fn lower_to_blocks_with_diagnostics(raw: &RawDoc) -> (AozoraAst, Vec<LowerDiagnostic>) {
    // 開いている Nested ブロックのビルダー（種類・たまった子ブロック列・開いた行番号）。
    let mut stack: Vec<(BlockKind, Vec<Block>, usize, OpenKind)> = Vec::new();
    let mut top: Vec<Block> = Vec::new();
    let mut diags: Vec<LowerDiagnostic> = Vec::new();

    for raw_line in &raw.lines {
        let line_no = raw_line.line_no;
        // 前方参照とルビ親文字を解決してから畳む（旧経路と同順）。span は畳み込みに使わない。
        let mut nodes = raw_line.nodes.clone();
        resolve_references(&mut nodes);
        resolve_inline_ruby(&mut nodes);

        match classify_line(&nodes) {
            LineKind::BlockOpen(kind) => {
                // 参照実装 close_conflicting_blocks の implicit_close を再現する。
                //  - Jisage 開始: 最上位が Jisage/Burasage なら1つ閉じる。
                //  - Chitsuki 開始: 最上位から Chitsuki/Burasage が続く限り閉じる。
                //  - Burasage 開始: 最上位から Jisage/Burasage が続く限り閉じる。
                // 閉じタグ直後の改行: 開始タグを即座に出すブロック（Jisage/Chitsuki 等）は
                // `</div><新開始…>` と同じ出力行に続くので改行なし（explicit_close=false）。
                // Burasage は開始行に可視タグを出さない per-line モデルなので、暗黙閉じの
                // `</div>` はその開始行の唯一の出力＝行末 `\r\n` が付く（explicit_close=true）。
                let (matches_top, close): (fn(&BlockKind) -> bool, CloseKind) = match &kind {
                    BlockKind::Jisage { .. } => (is_jisage_or_burasage, CloseKind::NoBreak),
                    BlockKind::Chitsuki { .. } => (is_chitsuki_or_burasage, CloseKind::NoBreak),
                    BlockKind::Burasage { .. } => (is_jisage_or_burasage, CloseKind::Newline),
                    _ => (never_matches, CloseKind::NoBreak),
                };
                // Jisage は1つだけ、Chitsuki/Burasage は続く限り閉じる。
                let close_once = matches!(kind, BlockKind::Jisage { .. });
                while stack.last().is_some_and(|(top, _, _, _)| matches_top(top)) {
                    let (k, children, open_line, open) = stack.pop().expect("top exists");
                    push_block(
                        &mut stack,
                        &mut top,
                        Block::Nested {
                            kind: k,
                            children,
                            close,
                            open,
                            line: open_line,
                        },
                    );
                    if close_once {
                        break;
                    }
                }
                stack.push((kind, Vec::new(), line_no, OpenKind::Newline));
            }
            LineKind::BlockOpenWithTail(idx, kind) => {
                // 開始タグより前の本文は開くブロックの外に出る。改行は開始タグ以降が
                // 出すので Break::NoNewline。開始タグ直後にも改行は出ない（OpenKind）。
                // 対象は Shatai/Caption 等で、暗黙閉じ（Jisage/Chitsuki/Burasage）は
                // 行頭で開く形でしか現れないのでここでは扱わない。
                if idx > 0 {
                    push_block(
                        &mut stack,
                        &mut top,
                        Block::Line {
                            inline: crate::ast::to_inlines(&nodes[..idx]),
                            brk: Break::NoNewline,
                            line: line_no,
                        },
                    );
                }
                stack.push((kind, Vec::new(), line_no, OpenKind::NoBreak));
                let inline = crate::ast::to_inlines(&nodes[idx + 1..]);
                let brk = if crate::ast::line_is_block_only(&inline) {
                    Break::None
                } else {
                    Break::Br
                };
                push_block(
                    &mut stack,
                    &mut top,
                    Block::Line {
                        inline,
                        brk,
                        line: line_no,
                    },
                );
            }
            LineKind::BlockClose(explicit) => {
                if let Some((kind, children, open_line, open)) = stack.pop() {
                    // `ここで…終わり`（explicit）は `</div>\r\n`。bare `…終わり` は
                    // @terprip 維持で `</div><br />\r\n`（memory bare-block-end）。
                    let mut close = if explicit {
                        CloseKind::Newline
                    } else {
                        CloseKind::BareBreak
                    };
                    // ぶら下げの直下で装飾系ブロックが閉じる行は、参照が閉じタグを
                    // String 扱いして per-line の burasage div で包む。包む幅は外側の
                    // ぶら下げが持つので、ここで畳んで木に載せる（描画器は状態を持たない）。
                    if is_burasage_wrapped_close(&kind) {
                        if let Some((BlockKind::Burasage { wrap_width, width }, _, _, _)) =
                            stack.last()
                        {
                            close = CloseKind::BurasageWrapped {
                                wrap_width: *wrap_width,
                                width: *width,
                            };
                        }
                    }
                    let nested = Block::Nested {
                        kind,
                        children,
                        close,
                        open,
                        line: open_line,
                    };
                    push_block(&mut stack, &mut top, nested);
                }
                // 対応する開きが無ければ捨てる（旧経路も未マッチ終了は無出力）。
            }
            LineKind::BlockCloseWithTail(explicit) => {
                // ブロックを閉じる（`</div>` 改行なし＝explicit_close=false）。閉じの
                // `\r\n` は後続本文行が出す。開きが無ければ閉じタグは出ない。
                if let Some((kind, children, open_line, open)) = stack.pop() {
                    push_block(
                        &mut stack,
                        &mut top,
                        Block::Nested {
                            kind,
                            children,
                            close: CloseKind::NoBreak,
                            open,
                            line: open_line,
                        },
                    );
                }
                // 先頭 BlockEnd を除いた後続本文を同じ行に出す（explicit なら br 抑制）。
                let rest: Vec<Node> = nodes.into_iter().skip(1).collect();
                let inline = crate::ast::to_inlines(&rest);
                let brk = if explicit || crate::ast::line_is_block_only(&inline) {
                    Break::None
                } else {
                    Break::Br
                };
                push_block(
                    &mut stack,
                    &mut top,
                    Block::Line {
                        inline,
                        brk,
                        line: line_no,
                    },
                );
            }
            LineKind::BlockCloseWithHead(idx, explicit) if stack.is_empty() => {
                // 対応する開きが無ければ閉じタグは出ない。行をまとめて内容行にする。
                let _ = idx;
                let inline = crate::ast::to_inlines(&nodes);
                let brk = if explicit || crate::ast::line_is_block_only(&inline) {
                    Break::None
                } else {
                    Break::Br
                };
                push_block(
                    &mut stack,
                    &mut top,
                    Block::Line {
                        inline,
                        brk,
                        line: line_no,
                    },
                );
            }
            LineKind::BlockCloseWithHead(idx, explicit) => {
                // 参照は行を逐次出力するので、閉じタグより前の本文は閉じるブロックの
                // 内側に出る。行末の改行は閉じタグ（後続本文があればその行）が出すので
                // 前半は Break::NoNewline。
                let tail: Vec<Node> = nodes[idx + 1..].to_vec();
                let head_inline = crate::ast::to_inlines(&nodes[..idx]);
                push_block(
                    &mut stack,
                    &mut top,
                    Block::Line {
                        inline: head_inline,
                        brk: Break::NoNewline,
                        line: line_no,
                    },
                );
                if let Some((kind, children, open_line, open)) = stack.pop() {
                    // 後続本文があるなら改行はその行が出すので `</div>` のみ。
                    let close = if !tail.is_empty() {
                        CloseKind::NoBreak
                    } else if explicit {
                        CloseKind::Newline
                    } else {
                        CloseKind::BareBreak
                    };
                    push_block(
                        &mut stack,
                        &mut top,
                        Block::Nested {
                            kind,
                            children,
                            close,
                            open,
                            line: open_line,
                        },
                    );
                }
                if !tail.is_empty() {
                    let inline = crate::ast::to_inlines(&tail);
                    let brk = if explicit || crate::ast::line_is_block_only(&inline) {
                        Break::None
                    } else {
                        Break::Br
                    };
                    push_block(
                        &mut stack,
                        &mut top,
                        Block::Line {
                            inline,
                            brk,
                            line: line_no,
                        },
                    );
                }
            }
            LineKind::LineWrap(kind) => {
                // ［＃N字下げ］text／行スコープ地付き: 行全体を div で1行に包む。
                // 先頭の行スコープマーカー1個（LineJisage、または is_block=false の
                // 行スコープ BlockStart）だけを取り除き、残りは to_inlines に渡す
                // （行内の見出しコマンド範囲などはそちらが畳む）。
                let rest = strip_leading_line_scope_marker(nodes);
                let inline = crate::ast::to_inlines(&rest);
                push_block(
                    &mut stack,
                    &mut top,
                    Block::LineWrap {
                        kind,
                        inline,
                        line: line_no,
                    },
                );
            }
            LineKind::Content => {
                // ［＃ここで…終わり］（explicit_close=true）を含む行は @terprip=false で
                // 行末 <br /> を抑制する（同行開閉の横組み等・複数行ブロックの閉じ行）。
                let has_explicit_close = nodes.iter().any(|n| {
                    matches!(
                        &n.kind,
                        NodeKind::BlockEnd {
                            explicit_close: true,
                            ..
                        }
                    )
                });
                let inline = crate::ast::to_inlines(&nodes);
                // 行末 <br /> の要否を Lower 時に確定（@terprip：ここで…終わり行、
                // および見出し・ブロックのみ行では抑制）。
                let brk = if has_explicit_close || crate::ast::line_is_block_only(&inline) {
                    Break::None
                } else {
                    Break::Br
                };
                let line = Block::Line {
                    inline,
                    brk,
                    line: line_no,
                };
                push_block(&mut stack, &mut top, line);
            }
        }
    }

    // 閉じられていないブロックはそのまま閉じる（旧経路の末尾 pop 相当）。
    // 末尾クローズは行を持たないので `</div>\r\n`（Newline）とする。
    while let Some((kind, children, open_line, open)) = stack.pop() {
        // EOF まで対応する「終わり」が現れなかった＝閉じ忘れの可能性。診断に記録する
        // （出力は従来どおり末尾クローズ。診断は追加返却のみで Block 出力は不変）。
        diags.push(LowerDiagnostic {
            line: open_line,
            kind: kind.clone(),
        });
        let nested = Block::Nested {
            kind,
            children,
            line: open_line,
            close: CloseKind::Newline,
            open,
        };
        push_block(&mut stack, &mut top, nested);
    }

    (top, diags)
}

/// 行スコープ包みの先頭マーカー1個を取り除いた残りのノード列を返す。
///
/// ［＃N字下げ］（`LineJisage`、行内どこでも）を1個、または先頭の行スコープ
/// `BlockStart`（is_block=false の Jisage/Chitsuki＝地付き）を取り除く。行内の
/// 見出しコマンド範囲などブロックマーカーはそのまま残す（to_inlines が畳む）。
fn strip_leading_line_scope_marker(nodes: Vec<Node>) -> Vec<Node> {
    // まず LineJisage を1個だけ落とす（参照 apply_jisage の位置除去）。
    if let Some(pos) = nodes
        .iter()
        .position(|n| matches!(&n.kind, NodeKind::LineJisage { .. }))
    {
        let mut rest = nodes;
        rest.remove(pos);
        return rest;
    }
    // 先頭が行スコープ BlockStart（is_block=false の Jisage/Chitsuki）なら落とす。
    if let Some(Node {
        kind: NodeKind::BlockStart { block_type, params },
        ..
    }) = nodes.first()
    {
        if !params.is_block && matches!(block_type, BlockType::Jisage | BlockType::Chitsuki) {
            return nodes.into_iter().skip(1).collect();
        }
    }
    nodes
}

/// ぶら下げの中で閉じるとき、閉じタグが per-line の burasage div に包まれる種類か。
///
/// 参照実装 `is_decoration_block_close` 相当。字下げ・地付き・ぶら下げ自身・見出しは
/// 該当しない（それらの閉じは String を残さない）。
fn is_burasage_wrapped_close(k: &BlockKind) -> bool {
    matches!(
        k,
        BlockKind::Yokogumi
            | BlockKind::Keigakomi
            | BlockKind::Caption
            | BlockKind::FontSize { .. }
            | BlockKind::Futoji
            | BlockKind::Shatai
            | BlockKind::Jizume { .. }
    )
}

fn is_jisage_or_burasage(k: &BlockKind) -> bool {
    matches!(k, BlockKind::Jisage { .. } | BlockKind::Burasage { .. })
}

fn is_chitsuki_or_burasage(k: &BlockKind) -> bool {
    matches!(k, BlockKind::Chitsuki { .. } | BlockKind::Burasage { .. })
}

fn never_matches(_: &BlockKind) -> bool {
    false
}

/// 現在開いている最上位ブロック（あれば）へ、無ければトップレベルへ block を積む。
fn push_block(
    stack: &mut [(BlockKind, Vec<Block>, usize, OpenKind)],
    top: &mut Vec<Block>,
    block: Block,
) {
    if let Some((_, children, _, _)) = stack.last_mut() {
        children.push(block);
    } else {
        top.push(block);
    }
}

/// 行の種類。
enum LineKind {
    /// ブロック開始（`ここから…`）。単独行の BlockStart(is_block=true)。
    BlockOpen(BlockKind),
    /// ブロック終了。単独行の BlockEnd。bool は explicit_close（`ここで…終わり`=true、
    /// bare `…終わり`=false）。
    BlockClose(bool),
    /// 先頭 BlockEnd＋後続本文の行（`［＃ここで…終わり］text`）。bool は explicit_close。
    BlockCloseWithTail(bool),
    /// 本文の後ろに BlockEnd が現れる行（`text［＃ここで…終わり］` および
    /// `text［＃ここで…終わり］text`）。usize は BlockEnd の位置、bool は
    /// explicit_close。参照実装は行を逐次出力するので、前半の本文を出してから
    /// その場でブロックを閉じ、後続の本文を同じ行に続ける。
    BlockCloseWithHead(usize, bool),
    /// 行の途中で開く複数行ブロック（`text［＃ここから斜体］text` / 行頭で開いて
    /// 本文が続く `［＃ここからキャプション］text`）。usize は BlockStart の位置。
    /// 参照は開始タグをその場に出し、同じ行に内容を続ける。
    BlockOpenWithTail(usize, BlockKind),
    /// 行スコープの1行包み（同行に本文あり）。字下げ／地付き。
    LineWrap(BlockKind),
    /// 内容行。
    Content,
}

/// 解決済みノード列から行の種類を判定する。
///
/// **最小核**: 単独の BlockStart(is_block=true) を開始、単独の BlockEnd を終了、
/// それ以外を内容行とみなす。コマンドと同行に本文があるケース・行単位字下げ
/// （LineJisage）・ぶら下げ per-line 等は今後の段で足す（TODO）。
fn classify_line(nodes: &[Node]) -> LineKind {
    if let [Node {
        kind: NodeKind::BlockStart { block_type, params },
        ..
    }] = nodes
    {
        if params.is_block {
            if let Some(kind) = block_kind_of(block_type, params) {
                return LineKind::BlockOpen(kind);
            }
        }
    }
    if let [Node {
        kind: NodeKind::BlockEnd { explicit_close, .. },
        ..
    }] = nodes
    {
        return LineKind::BlockClose(*explicit_close);
    }
    // 行の途中（または行頭で本文が続く形）で開く複数行ブロック。参照は開始タグを
    // その場に出して同じ行に内容を続ける。同じ行に対応する終わりがある範囲形は
    // `to_inlines` が BlockInline に畳むので、行内に BlockEnd が無い場合に限る。
    if let Some(idx) = nodes
        .iter()
        .position(|n| matches!(&n.kind, NodeKind::BlockStart { params, .. } if params.is_block))
    {
        let NodeKind::BlockStart { block_type, params } = &nodes[idx].kind else {
            unreachable!("position で BlockStart を選んでいる")
        };
        let has_tail = idx + 1 < nodes.len();
        let no_end_on_line = !nodes[idx + 1..]
            .iter()
            .any(|n| matches!(n.kind, NodeKind::BlockEnd { .. }));
        let head_is_text = nodes[..idx]
            .iter()
            .all(|n| matches!(n.kind, NodeKind::Text(_)));
        if has_tail && no_end_on_line && head_is_text {
            if let Some(kind) = block_kind_of(block_type, params) {
                return LineKind::BlockOpenWithTail(idx, kind);
            }
        }
    }
    // 先頭が BlockEnd で後続に本文がある行（`［＃ここで…終わり］　` 等）。参照は
    // ブロックを閉じ（`</div>` 改行なし）、続く本文をその行に出す（行末 br は
    // BlockEnd が explicit_close なら抑制）。
    if let Some((
        Node {
            kind: NodeKind::BlockEnd { explicit_close, .. },
            ..
        },
        rest,
    )) = nodes.split_first()
    {
        if !rest.is_empty() {
            return LineKind::BlockCloseWithTail(*explicit_close);
        }
    }
    // 本文（素のテキストのみ）の後ろに BlockEnd が現れる行。`［＃「…」は太字終わり］`
    // 等のインライン範囲終端を巻き込まないよう、前半が Text だけの場合に限る。
    if let Some(idx) = nodes
        .iter()
        .position(|n| matches!(n.kind, NodeKind::BlockEnd { .. }))
    {
        if idx > 0
            && nodes[..idx]
                .iter()
                .all(|n| matches!(n.kind, NodeKind::Text(_)))
        {
            let NodeKind::BlockEnd { explicit_close, .. } = nodes[idx].kind else {
                unreachable!("position で BlockEnd を選んでいる")
            };
            return LineKind::BlockCloseWithHead(idx, explicit_close);
        }
    }
    // 行単位字下げ ［＃N字下げ］。行にこのマーカーしか無ければ複数行ブロックを開く
    // （参照 apply_jisage の unshift 相当＝ここから字下げと同一）。本文が続けば行包み。
    if let [Node {
        kind: NodeKind::LineJisage { width },
        ..
    }] = nodes
    {
        return LineKind::BlockOpen(BlockKind::Jisage {
            width: Some(*width),
        });
    }
    if let Some(Node {
        kind: NodeKind::LineJisage { width },
        ..
    }) = nodes
        .iter()
        .find(|n| matches!(&n.kind, NodeKind::LineJisage { .. }))
    {
        return LineKind::LineWrap(BlockKind::Jisage {
            width: Some(*width),
        });
    }
    // 行スコープ地付き／字上げ ［＃地付き］text（先頭が is_block=false の Chitsuki）。
    // 参照 renderer は先頭ノードで判定し、行末でブロックを閉じる（1行包み）。
    if let Some(Node {
        kind: NodeKind::BlockStart { block_type, params },
        ..
    }) = nodes.first()
    {
        if !params.is_block && *block_type == BlockType::Chitsuki {
            return LineKind::LineWrap(BlockKind::Chitsuki {
                width: params.width.unwrap_or(0),
            });
        }
    }
    LineKind::Content
}

/// RawAST の BlockType＋params をAozora ASTの BlockKind に写す（対応済みのものだけ）。
pub(crate) fn block_kind_of(
    block_type: &BlockType,
    params: &crate::node::BlockParams,
) -> Option<BlockKind> {
    let w = || params.width.unwrap_or(0);
    match block_type {
        BlockType::Jisage => Some(BlockKind::Jisage {
            width: params.width,
        }),
        BlockType::Chitsuki => Some(BlockKind::Chitsuki { width: w() }),
        BlockType::Jizume => Some(BlockKind::Jizume { width: w() }),
        BlockType::Keigakomi => Some(BlockKind::Keigakomi),
        BlockType::Yokogumi => Some(BlockKind::Yokogumi),
        BlockType::Caption => Some(BlockKind::Caption),
        BlockType::FontDai => Some(BlockKind::FontSize {
            size_type: crate::node::FontSizeType::Dai,
            level: params.font_size.unwrap_or(1),
        }),
        BlockType::FontSho => Some(BlockKind::FontSize {
            size_type: crate::node::FontSizeType::Sho,
            level: params.font_size.unwrap_or(1),
        }),
        BlockType::Futoji => Some(BlockKind::Futoji),
        BlockType::Shatai => Some(BlockKind::Shatai),
        BlockType::Burasage => Some(BlockKind::Burasage {
            wrap_width: params.wrap_width,
            width: params.width,
        }),
        BlockType::Midashi => Some(BlockKind::Midashi {
            level: params.level.unwrap_or(crate::node::MidashiLevel::O),
            style: params
                .midashi_style
                .unwrap_or(crate::node::MidashiStyle::Normal),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod position_tests {
    use super::*;
    use crate::ast::Block;
    use crate::parser::parse_document_raw;

    /// Aozora ASTの各ブロックが由来の本文行番号（位置情報）を持つ。
    #[test]
    fn blocks_carry_source_line_numbers() {
        // 本文（extract 後を模した行列）。0 起点で数える。
        let lines = vec![
            "本文0",                    // 0
            "［＃ここから２字下げ］",   // 1 (Nested open)
            "内容2",                    // 2
            "［＃ここで字下げ終わり］", // 3
        ];
        let raw = parse_document_raw(&lines);
        let blocks = lower_to_blocks(&raw);
        // [ Line(本文0, line0), Nested(open line1, 子[Line(内容2, line2)]) ]
        assert!(matches!(blocks[0], Block::Line { line: 0, .. }));
        match &blocks[1] {
            Block::Nested { line, children, .. } => {
                assert_eq!(*line, 1, "Nested は開いた行1");
                assert!(matches!(children[0], Block::Line { line: 2, .. }));
            }
            other => panic!("Nested を期待: {other:?}"),
        }
    }

    /// 本文の途中（行末）に現れた「ここで…終わり」もその場でブロックを閉じる。
    ///
    /// 参照実装は行を逐次出力するので、閉じタグより前の本文は閉じるブロックの内側に
    /// 出て、行末の改行は閉じタグが出す。現実の入力では CRLF 区切りの行に孤立 LF が
    /// 混ざる形で現れる（例: 宮本百合子「千世子」000311/15945 の1箇所が
    /// `"\n［＃ここで字下げ終わり］"`）。
    #[test]
    fn block_end_after_text_closes_the_block_on_that_line() {
        let lines = vec![
            "［＃ここから１字下げ］",
            "内容",
            "\n［＃ここで字下げ終わり］",
            "後続",
        ];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        match &blocks[0] {
            Block::Nested {
                children, close, ..
            } => {
                assert_eq!(children.len(), 2, "本文行と行途中クローズ前の本文");
                assert!(matches!(
                    children[1],
                    Block::Line {
                        brk: Break::NoNewline,
                        ..
                    }
                ));
                assert_eq!(*close, CloseKind::Newline);
            }
            other => panic!("Nested を期待: {other:?}"),
        }
        assert!(matches!(blocks[1], Block::Line { line: 3, .. }));
    }

    /// 行途中クローズの後ろに本文が続く場合、閉じタグは `</div>`（改行なし）で、
    /// 行末の改行は後続本文が出す（例: 000081/48220 の
    /// `（正方形にやりますか。）［＃ここで字下げ終わり］どういふ訳か…`）。
    #[test]
    fn block_end_between_texts_closes_and_continues_on_the_same_line() {
        let lines = vec!["［＃ここから４字下げ］", "前［＃ここで字下げ終わり］後"];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        match &blocks[0] {
            Block::Nested { close, .. } => assert_eq!(*close, CloseKind::NoBreak),
            other => panic!("Nested を期待: {other:?}"),
        }
        // 後続本文は同じ行として `\r\n` を出す（explicit なので `<br />` は抑制）。
        assert!(matches!(
            blocks[1],
            Block::Line {
                brk: Break::None,
                line: 1,
                ..
            }
        ));
    }

    /// 行の途中で開く複数行ブロックは、開始タグをその場に出して同じ行に内容を
    /// 続ける（開始タグ直後に改行を出さない＝`OpenKind::NoBreak`）。
    ///
    /// 例: 001065/18361 の `　［＃ここから斜体］Fourscore and seven…`、
    /// 001841/57318 の `［＃ここからキャプション］図３　ペラグラ患者。`
    #[test]
    fn block_start_mid_line_opens_and_continues_on_the_same_line() {
        let lines = vec!["　［＃ここから斜体］前半", "後半［＃ここで斜体終わり］"];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        // 開始タグより前の本文はブロックの外・改行なし。
        assert!(matches!(
            blocks[0],
            Block::Line {
                brk: Break::NoNewline,
                ..
            }
        ));
        match &blocks[1] {
            Block::Nested {
                kind,
                children,
                open,
                ..
            } => {
                assert_eq!(*kind, BlockKind::Shatai);
                assert_eq!(*open, OpenKind::NoBreak);
                assert_eq!(children.len(), 2, "同行の後続本文と次行の本文");
            }
            other => panic!("Nested を期待: {other:?}"),
        }
    }

    /// 同じ行で開閉する範囲形は `to_inlines` が BlockInline に畳むので、
    /// ブロックとしては開かない。
    #[test]
    fn block_range_closed_on_the_same_line_stays_inline() {
        let blocks = lower_to_blocks(&parse_document_raw(&[
            "前［＃ここから斜体］中［＃ここで斜体終わり］後",
        ]));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], Block::Line { .. }));
    }

    /// 対応する開きが無ければ閉じタグは出さず、通常の内容行として扱う。
    #[test]
    fn block_end_after_text_without_open_block_is_plain_content() {
        let blocks = lower_to_blocks(&parse_document_raw(&["本文［＃ここで字下げ終わり］"]));
        assert_eq!(blocks.len(), 1);
        assert!(matches!(
            blocks[0],
            Block::Line {
                brk: Break::None,
                ..
            }
        ));
    }
}
