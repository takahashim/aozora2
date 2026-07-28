//! 行末の改行制御（[`crate::ast::Break`]）を決める規則。
//!
//! 参照実装 `general_output` は、その行の出力が「ブロックの閉じタグで終わる」
//! （`</div>` や `</hN>`）とき `@terprip` を倒して行末の `<br />` を出さない。
//! 描画済み HTML を見て決めていたその判断を、Lower 時にインライン列から決める形に
//! 移したもの（バックエンドの HTML 詮索を無くす。architecture.md §4.3）。
//!
//! 判断の中身は参照実装の HTML 出力形に由来するので、backend-neutral な型を置く
//! [`crate::ast`] ではなく Lowerer 側に置く。

use crate::ast::{BlockKind, Inline, InlineKind};
use crate::node::MidashiStyle;

/// この行のインライン列を描画すると、行末がブロックの閉じタグ（`</div>`/`</hN>`）に
/// なるか。参照実装が行末 `<br />` を抑制する形かどうかの判定で、末尾のインライン
/// だけで決まる（参照は描画済みバッファの行末タグを見るため）。
pub fn line_emits_closing_block_tag(inlines: &[Inline]) -> bool {
    match inlines.last() {
        // 見出しは Normal のみ `</hN>`（dogyo-/mado- はインラインなので br）。
        Some(Inline {
            kind: InlineKind::Midashi { style, .. },
            ..
        }) => *style == MidashiStyle::Normal,
        // 行末で開いた地付き（`…</div>`）。
        Some(Inline {
            kind: InlineKind::ChitsukiInline { .. },
            ..
        }) => true,
        // 同行開閉のブロック形（div で包むか、Normal 見出し）。
        Some(Inline {
            kind: InlineKind::BlockInline { kind, .. },
            ..
        }) => block_kind_emits_closing_tag(kind),
        _ => false,
    }
}

/// `BlockInline` の種類が末尾 `</div>`/`</hN>` になるか。
fn block_kind_emits_closing_tag(kind: &BlockKind) -> bool {
    match kind {
        BlockKind::Midashi { style, .. } => *style == MidashiStyle::Normal,
        // div で包む種類は末尾が `</div>`。
        BlockKind::Jisage { .. }
        | BlockKind::Chitsuki { .. }
        | BlockKind::Jizume { .. }
        | BlockKind::Keigakomi
        | BlockKind::Yokogumi
        | BlockKind::Caption
        | BlockKind::FontSize { .. }
        | BlockKind::Futoji
        | BlockKind::Shatai => true,
        // Burasage は BlockInline には現れない。
        BlockKind::Burasage(_) => false,
    }
}
