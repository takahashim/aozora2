//! Lowerer: RawAST（平坦マーカー）→ 中立AST（block ⊃ line ⊃ inline の木）
//!
//! architecture.md §4.1/§4.3・docs/plan-neutral-ast.md（Phase B2〜）。
//! 参照実装 `@indent_stack`/`implicit_close`/`@terprip` の逐次モデルを Lower 時に
//! 一度だけ計算し、ブロックを部分木に畳み、行末の改行を [`Break`] メタデータへ
//! 載せる。バックエンドはこの木を状態なしに歩くだけになる。
//!
//! **段階実装中**: まず jisage と内容行だけを畳む（垂直スライスの最小核）。
//! 旧 BlockManager 経路と本文HTMLが byte 一致することを確認しながら記法を1種類ずつ
//! 増やす。未対応のブロック種は暫定でトップレベルに落とす（TODO）。

use crate::ast::{Block, BlockKind, Break};
use crate::node::{BlockType, Node};
use crate::parser::reference_resolver::{resolve_inline_ruby, resolve_references};
use crate::parser::RawDoc;

/// RawDoc（未解決・平坦マーカー）を中立ASTのブロック列に畳む。
pub fn lower_to_blocks(raw: &RawDoc) -> Vec<Block> {
    // 開いている Nested ブロックのビルダー（種類と、たまった子ブロック列）。
    let mut stack: Vec<(BlockKind, Vec<Block>)> = Vec::new();
    let mut top: Vec<Block> = Vec::new();

    for raw_line in &raw.lines {
        // 前方参照とルビ親文字を解決してから畳む（旧経路と同順）。
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
                let (matches_top, close_nl): (fn(&BlockKind) -> bool, bool) = match &kind {
                    BlockKind::Jisage { .. } => (is_jisage_or_burasage, false),
                    BlockKind::Chitsuki { .. } => (is_chitsuki_or_burasage, false),
                    BlockKind::Burasage { .. } => (is_jisage_or_burasage, true),
                    _ => (never_matches, false),
                };
                // Jisage は1つだけ、Chitsuki/Burasage は続く限り閉じる。
                let close_once = matches!(kind, BlockKind::Jisage { .. });
                while stack.last().is_some_and(|(top, _)| matches_top(top)) {
                    let (k, children) = stack.pop().expect("top exists");
                    push_block(
                        &mut stack,
                        &mut top,
                        Block::Nested {
                            kind: k,
                            children,
                            explicit_close: close_nl,
                        },
                    );
                    if close_once {
                        break;
                    }
                }
                stack.push((kind, Vec::new()));
            }
            LineKind::BlockClose => {
                if let Some((kind, children)) = stack.pop() {
                    // `ここで…終わり` による明示閉じ（`</div>\r\n`）。
                    let nested = Block::Nested {
                        kind,
                        children,
                        explicit_close: true,
                    };
                    push_block(&mut stack, &mut top, nested);
                }
                // 対応する開きが無ければ捨てる（旧経路も未マッチ終了は無出力）。
            }
            LineKind::LineWrap(kind) => {
                // ［＃N字下げ］text／行スコープ地付き: 行全体を div で1行に包む。
                // 先頭の行スコープマーカー1個（LineJisage、または is_block=false の
                // 行スコープ BlockStart）だけを取り除き、残りは to_inlines に渡す
                // （行内の見出しコマンド範囲などはそちらが畳む）。
                let rest = strip_leading_line_scope_marker(nodes);
                let inline = crate::ast::to_inlines(&rest);
                push_block(&mut stack, &mut top, Block::LineWrap { kind, inline });
            }
            LineKind::Content => {
                // ［＃ここで…終わり］（explicit_close=true）を含む行は @terprip=false で
                // 行末 <br /> を抑制する（同行開閉の横組み等・複数行ブロックの閉じ行）。
                let has_explicit_close = nodes.iter().any(|n| {
                    matches!(
                        n,
                        Node::BlockEnd {
                            explicit_close: true,
                            ..
                        }
                    )
                });
                let brk = if has_explicit_close {
                    Break::None
                } else {
                    Break::Br
                };
                let inline = crate::ast::to_inlines(&nodes);
                let line = Block::Line { inline, brk };
                push_block(&mut stack, &mut top, line);
            }
        }
    }

    // 閉じられていないブロックはそのまま閉じる（旧経路の末尾 pop 相当）。
    // 末尾クローズは行を持たないので explicit_close=true（`</div>\r\n`）とする。
    while let Some((kind, children)) = stack.pop() {
        let nested = Block::Nested {
            kind,
            children,
            explicit_close: true,
        };
        push_block(&mut stack, &mut top, nested);
    }

    top
}

/// 行スコープ包みの先頭マーカー1個を取り除いた残りのノード列を返す。
///
/// ［＃N字下げ］（`LineJisage`、行内どこでも）を1個、または先頭の行スコープ
/// `BlockStart`（is_block=false の Jisage/Chitsuki＝地付き）を取り除く。行内の
/// 見出しコマンド範囲などブロックマーカーはそのまま残す（to_inlines が畳む）。
fn strip_leading_line_scope_marker(nodes: Vec<Node>) -> Vec<Node> {
    // まず LineJisage を1個だけ落とす（参照 apply_jisage の位置除去）。
    if let Some(pos) = nodes.iter().position(|n| matches!(n, Node::LineJisage { .. })) {
        let mut rest = nodes;
        rest.remove(pos);
        return rest;
    }
    // 先頭が行スコープ BlockStart（is_block=false の Jisage/Chitsuki）なら落とす。
    if let Some(Node::BlockStart { block_type, params }) = nodes.first() {
        if !params.is_block
            && matches!(block_type, BlockType::Jisage | BlockType::Chitsuki)
        {
            return nodes.into_iter().skip(1).collect();
        }
    }
    nodes
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
fn push_block(stack: &mut [(BlockKind, Vec<Block>)], top: &mut Vec<Block>, block: Block) {
    if let Some((_, children)) = stack.last_mut() {
        children.push(block);
    } else {
        top.push(block);
    }
}

/// 行の種類。
enum LineKind {
    /// ブロック開始（`ここから…`）。単独行の BlockStart(is_block=true)。
    BlockOpen(BlockKind),
    /// ブロック終了（`ここで…終わり`）。単独行の BlockEnd。
    BlockClose,
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
    if let [Node::BlockStart { block_type, params }] = nodes {
        if params.is_block {
            if let Some(kind) = block_kind_of(block_type, params) {
                return LineKind::BlockOpen(kind);
            }
        }
    }
    if let [Node::BlockEnd { .. }] = nodes {
        return LineKind::BlockClose;
    }
    // 行単位字下げ ［＃N字下げ］。行にこのマーカーしか無ければ複数行ブロックを開く
    // （参照 apply_jisage の unshift 相当＝ここから字下げと同一）。本文が続けば行包み。
    if let [Node::LineJisage { width }] = nodes {
        return LineKind::BlockOpen(BlockKind::Jisage { width: *width });
    }
    if let Some(Node::LineJisage { width }) = nodes
        .iter()
        .find(|n| matches!(n, Node::LineJisage { .. }))
    {
        return LineKind::LineWrap(BlockKind::Jisage { width: *width });
    }
    // 行スコープ地付き／字上げ ［＃地付き］text（先頭が is_block=false の Chitsuki）。
    // 参照 renderer は先頭ノードで判定し、行末でブロックを閉じる（1行包み）。
    if let Some(Node::BlockStart { block_type, params }) = nodes.first() {
        if !params.is_block && *block_type == BlockType::Chitsuki {
            return LineKind::LineWrap(BlockKind::Chitsuki {
                width: params.width.unwrap_or(0),
            });
        }
    }
    LineKind::Content
}

/// RawAST の BlockType＋params を中立ASTの BlockKind に写す（対応済みのものだけ）。
pub(crate) fn block_kind_of(
    block_type: &BlockType,
    params: &crate::node::BlockParams,
) -> Option<BlockKind> {
    let w = || params.width.unwrap_or(0);
    match block_type {
        BlockType::Jisage => Some(BlockKind::Jisage { width: w() }),
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
        BlockType::Burasage => {
            // 参照 generate_burasage_start: wrap_width 既定 1、text_indent = width - wrap_width。
            let wrap_width = params.wrap_width.unwrap_or(1);
            let width = params.width.unwrap_or(0) as i32;
            Some(BlockKind::Burasage {
                wrap_width,
                text_indent: width - wrap_width as i32,
            })
        }
        BlockType::Midashi => Some(BlockKind::Midashi {
            level: params.level.unwrap_or(crate::node::MidashiLevel::O),
            style: params
                .midashi_style
                .unwrap_or(crate::node::MidashiStyle::Normal),
        }),
        _ => None,
    }
}
