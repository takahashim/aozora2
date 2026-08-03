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
    /// 出力を持たない行（制約 6 の `suppressed_by`）。昇順。
    pub suppressed_lines: Vec<usize>,
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
    /// この行へ結合されてきた断片の由来（制約 6 の `joins_before`）。昇順で、
    /// いずれも [`PlanLine::line`] より前。結合が無ければ空。
    ///
    /// AST には出ない説明用の情報で、[`materialize`] は捨てる。
    /// [`check_plan_invariants`] が結合の非巡回性をここで見る。
    pub source_lines: Vec<usize>,
}

/// [`LowerPlan`] を Aozora AST へ写す。純関数（判断をしない）。
pub(super) fn materialize(plan: LowerPlan) -> (AozoraAst, Vec<LowerDiagnostic>) {
    let ast = plan.roots.into_iter().map(materialize_block).collect();
    (ast, plan.diagnostics)
}

/// `LowerPlan` が満たすべき性質を検査する（`docs/spec-lowerer-constraints.md`）。
///
/// 解決器の走査順を変えても壊れてはいけない性質だけを見る。[`solve`](super::solve) の
/// 末尾から `cfg!(debug_assertions)` のときだけ呼ぶので、デバッグビルドで走る検証
/// （ユニットテスト・統合テスト・並走テスト）すべてがこの検査を通る。
///
/// 1. 包含（制約 3）: ブロックの開始行は、その中に現れるどの行番号よりも後ろにならない。
///    区間が入れ子か互いに素であることは森の形が保証するので、ここでは位置との整合を見る。
/// 2. 結合の非巡回（制約 6）: 結合元は昇順で、いずれも結合先の行より前。
/// 3. 診断の順（制約 8）: EOF 閉じは内側から順なので、行番号は非増加。
/// 4. 吸収した行（制約 6）: 昇順・重複なしで、出力行としては現れない。
pub(super) fn check_plan_invariants(plan: &LowerPlan) {
    let mut emitted = Vec::new();
    for block in &plan.roots {
        check_block(block, &mut emitted);
    }

    assert!(
        plan.diagnostics.windows(2).all(|w| w[0].line >= w[1].line),
        "診断は内側から順（行番号は非増加）のはず: {:?}",
        plan.diagnostics.iter().map(|d| d.line).collect::<Vec<_>>()
    );
    assert!(
        plan.suppressed_lines.windows(2).all(|w| w[0] < w[1]),
        "吸収した行は昇順・重複なしのはず: {:?}",
        plan.suppressed_lines
    );
    for line in &plan.suppressed_lines {
        assert!(
            !emitted.contains(line),
            "吸収した行 {line} が出力に現れている"
        );
    }
}

/// ブロックを歩いて行番号を集めつつ、包含と結合を検査する。
fn check_block(block: &PlanBlock, emitted: &mut Vec<usize>) {
    match block {
        PlanBlock::Content(line) => {
            check_line(line);
            emitted.push(line.line);
        }
        PlanBlock::LineWrap { line, .. } => emitted.push(*line),
        PlanBlock::Nested {
            opened_at,
            children,
            ..
        } => {
            let mut inner = Vec::new();
            for child in children {
                check_block(child, &mut inner);
            }
            if let Some(first) = inner.iter().min() {
                assert!(
                    opened_at <= first,
                    "ブロックは {opened_at} 行目で開いたのに、中に {first} 行目がある"
                );
            }
            emitted.extend(inner);
        }
    }
}

fn check_line(line: &PlanLine) {
    assert!(
        line.source_lines.windows(2).all(|w| w[0] < w[1]),
        "結合元は昇順のはず: {:?}",
        line.source_lines
    );
    if let Some(last) = line.source_lines.last() {
        assert!(
            *last < line.line,
            "結合元 {last} が結合先 {} より後ろにある（結合が巡回している）",
            line.line
        );
    }
}

fn materialize_block(block: PlanBlock) -> Block {
    match block {
        PlanBlock::Content(PlanLine {
            fragments,
            brk,
            line,
            // 説明用（AST には出ない）。
            source_lines: _,
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
