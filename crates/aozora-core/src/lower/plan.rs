//! 解決済みモデル [`LowerPlan`] と、そこから Aozora AST への写像 [`materialize`]。
//! `docs/spec-lowerer-constraints.md` の「LowerPlan」。
//!
//! `LowerPlan` は Lowerer 内部の型で、公開交換形式ではない。制約を解いた結果
//! （ブロック森・内容断片・`OpenKind`/`CloseKind`/`Break`・診断）がここに集まり、
//! [`materialize`] はそれを**状態を持たずに**歩いて木へ写すだけになる。判断は
//! すべて `solve` 側にあり、この関数には条件分岐を持ち込まない。

use super::LowerDiagnostic;
use crate::ast::{AozoraAst, Block, BlockKind, Break, CloseKind, Inline, OpenKind};

/// 解決結果。
pub(super) struct LowerPlan {
    /// どのブロックにも属さないトップレベルの並び。
    pub roots: Vec<PlanBlock>,
    /// EOF で閉じられなかったブロックの診断（内側から順）。
    pub diagnostics: Vec<LowerDiagnostic>,
}

/// 解決済みのブロック。
pub(super) enum PlanBlock {
    /// 内容の 1 行。
    Content(PlanLine),
    /// 複数行ブロック。
    Nested {
        kind: BlockKind,
        open: OpenKind,
        close: CloseKind,
        /// このブロックを開いた本文行（0 起点）。
        opened_at: usize,
        children: Vec<PlanBlock>,
    },
    /// 行スコープの 1 行包み（外側→内側の順）。
    ///
    /// 設計文書は `LineWrap { kinds, line: PlanLine }` としているが、AST の
    /// [`Block::LineWrap`] は行末改行を持たない（包みそのものが 1 行を成す）ので、
    /// ここでは [`Break`] を持たない形にしてある。持たせても materialize で捨てるだけで、
    /// 捨てられる値は後段の導出（制約 7）を惑わせる。
    LineWrap {
        kinds: Vec<BlockKind>,
        inline: Vec<Inline>,
        line: usize,
    },
}

/// 解決済みの内容行。
pub(super) struct PlanLine {
    /// 結合済みのインライン列。
    pub fragments: Vec<Inline>,
    /// 行末の改行（制約 7）。
    pub brk: Break,
    /// 出力上の行番号（0 起点）。結合が起きた行では**結合先の行**になる。
    pub line: usize,
}

/// [`LowerPlan`] を Aozora AST へ写す。純関数（判断をしない）。
pub(super) fn materialize(plan: LowerPlan) -> (AozoraAst, Vec<LowerDiagnostic>) {
    let ast = plan.roots.into_iter().map(materialize_block).collect();
    (ast, plan.diagnostics)
}

fn materialize_block(block: PlanBlock) -> Block {
    match block {
        PlanBlock::Content(PlanLine {
            fragments,
            brk,
            line,
        }) => Block::Line {
            inline: fragments,
            brk,
            line,
        },
        PlanBlock::Nested {
            kind,
            open,
            close,
            opened_at,
            children,
        } => Block::Nested {
            kind,
            children: children.into_iter().map(materialize_block).collect(),
            close,
            open,
            line: opened_at,
        },
        PlanBlock::LineWrap {
            kinds,
            inline,
            line,
        } => Block::LineWrap {
            kinds,
            inline,
            line,
        },
    }
}
