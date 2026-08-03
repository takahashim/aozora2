//! 行の事実（役割の割り当て）。`docs/spec-lowerer-constraints.md` の**制約 1**。
//!
//! 解決済みノード列 1 行から、行をまたぐ状態を持たずに [`LineFacts`] を作る。ここには
//! 純関数しか置かない——行の役割は「その行のマーカー全体」だけで決まり、開いている
//! ブロックにも持ち越しにも依存しない。
//!
//! [`LineRole`] の並びは制約 1 の規則 1〜7 と**同型**で、[`line_facts`] は上から順に
//! 試して最初に当たったものを返す。**判定順そのものが仕様**なので、並べ替えてはいけない。

use super::block_kind_of;
use crate::ast::BlockKind;
use crate::node::{BlockType, Node, NodeKind};

/// 行の役割（制約 1 の規則 1〜7）。
pub(super) enum LineRole {
    /// 規則 1: ぶら下げ開始行。行に終端マーカーが一つも無く、`is_block` のぶら下げ開始が
    /// ある（位置は問わない）。usize は開始マーカーの位置で、その前後の内容は結合断片に
    /// なる（制約 6）。
    BurasageOpen { marker: usize, kind: BlockKind },
    /// 規則 2: 単独の開始（`ここから…` 1 個だけからなる行）。規則 6 の例外
    /// （`［＃N字下げ］` 1 個だけからなる行）もここへ来る。
    BlockOpen(BlockKind),
    /// 規則 3: 単独の終端（割り注を除く）。bool は explicit_close（`ここで…終わり`=true、
    /// bare `…終わり`=false）。`block_kind_of` で写せない種類もここでは終端として扱う。
    BlockClose(bool),
    /// 規則 4: 未対応の終端を含む行。要素は (BlockEnd の位置, explicit_close) で、
    /// 現れる順に並ぶ。この行の開始は開かない。
    Closes(Vec<(usize, bool)>),
    /// 規則 5: 行途中の開始。usize は BlockStart の位置。
    BlockOpenWithTail(usize, BlockKind),
    /// 規則 6: 行スコープ包み（外側→内側の順）。
    LineWrap(Vec<BlockKind>),
    /// 規則 7: 内容行。
    Content,
}

/// 行から読み取れる、行をまたがない事実。
pub(super) struct LineFacts {
    /// 制約 1 が割り当てた役割。
    pub role: LineRole,
    /// 行が素の改行（[`NodeKind::UnclosedAccentBreak`]）で終わるか。内容行なら次の行と
    /// 1 つの出力単位になる（制約 6）。
    pub hard_break: bool,
}

/// 解決済みノード列から行の事実を作る。
pub(super) fn line_facts(nodes: &[Node]) -> LineFacts {
    LineFacts {
        role: role_of(nodes),
        hard_break: ends_with_hard_break(nodes),
    }
}

/// 制約 1 の規則を上から順に試す。
fn role_of(nodes: &[Node]) -> LineRole {
    // 規則 1: ぶら下げ開始行。
    //
    // 参照 `apply_burasage` は開始タグの位置に関係なく `@noprint` を立てるので、
    // 単独行でも同行に本文が続く形でも同じ扱いになる。規則 5 と違い「マーカーの
    // **後ろ**に要素がある」ことを求めないのはこのため。行末にマーカーがある
    // `\n［＃ここから改行天付き、折り返して１字下げ］`（本文中の裸 LF）も開く
    // （実文書 001240/46361）。
    if let Some((marker, kind)) = burasage_open(nodes) {
        return LineRole::BurasageOpen { marker, kind };
    }
    // 規則 2: 単独の開始。
    if let [Node {
        kind: NodeKind::BlockStart { block_type, params },
        ..
    }] = nodes
    {
        if params.is_block {
            if let Some(kind) = block_kind_of(block_type, params) {
                return LineRole::BlockOpen(kind);
            }
        }
    }
    // 規則 3: 単独の終端。
    if let [Node {
        kind:
            NodeKind::BlockEnd {
                block_type,
                explicit_close,
                ..
            },
        ..
    }] = nodes
    {
        // 割り注の終わりはブロックを閉じない。参照 apply_warichu は indent_stack に
        // 触れず `）</span>` を出力するだけなので、内容行として to_inlines へ流す。
        // 単独行 `［＃割り注終わり］` を素朴に「終わり」として扱うと、外側の字下げを
        // 閉じたうえに割り注の出力も落としていた（実測: 参照は字下げを閉じずに
        // `）</span><br />` を出す）。行途中の同じ形は find_unmatched_block_ends が
        // block_kind_of で既に除いており、ここはその抜けを塞ぐ。
        //
        // 他の非ブロック種（縦中横・装飾・注記付き範囲）の単独終わり行は参照実装が
        // エラー停止する入力で正解が無いので、従来どおり最も内側を閉じる緩い回復に
        // 任せる（そちらは block_kind_of で除かない）。
        if *block_type != BlockType::Warichu {
            return LineRole::BlockClose(*explicit_close);
        }
    }
    // 規則 4: 未対応の終端を含む行（単独行は規則 3 で処理済み）。同じ行で開閉する
    // 範囲形は `to_inlines` がインラインに畳むので、対応する開始が同じ行に無いものだけ。
    // 規則 5 より先に見るのは、閉じてから開く行で閉じを落とさないため。
    let closes = find_unmatched_block_ends(nodes);
    if !closes.is_empty() {
        return LineRole::Closes(closes);
    }
    // 規則 5: 行途中の開始。参照は開始タグをその場に出して同じ行に内容を続ける。
    if let Some(idx) = nodes
        .iter()
        .position(|n| matches!(&n.kind, NodeKind::BlockStart { params, .. } if params.is_block))
    {
        let NodeKind::BlockStart { block_type, params } = &nodes[idx].kind else {
            unreachable!("position で BlockStart を選んでいる")
        };
        let has_tail = idx + 1 < nodes.len();
        // 「同じ行で閉じているか」を、種類を問わない BlockEnd の有無で見る
        // （inline.rs の find_matching_end は同種で対応を取る）。両者が食い違うのは
        // 別種の終わりが混ざる行（`text［＃ここから斜体］text［＃ここで太字終わり］`）
        // だけで、これは参照実装がエラーで停止する入力なので正解を決められない。
        let no_end_on_line = !nodes[idx + 1..]
            .iter()
            .any(|n| matches!(n.kind, NodeKind::BlockEnd { .. }));
        // 「開始より前がすべてテキスト」は**現行の互換バグ**である。参照は前に何が
        // あってもブロックを開く（ルビ・外字・傍点・行内地付きの 5 形で実測）。
        // 現行再現プロファイルではそのまま写し、移行後に独立した変更で外す
        // （docs/spec-lowerer-constraints.md「既知の非互換と将来の統一」＝
        // docs/plan-lowerer-migration.md の PR-B）。
        let head_is_text = nodes[..idx]
            .iter()
            .all(|n| matches!(n.kind, NodeKind::Text(_)));
        if has_tail && no_end_on_line && head_is_text {
            if let Some(kind) = block_kind_of(block_type, params) {
                return LineRole::BlockOpenWithTail(idx, kind);
            }
        }
    }
    // 規則 6 の例外: 行単位字下げ ［＃N字下げ］ 1 個だけからなる行は複数行ブロックを
    // 開く（参照 apply_jisage の unshift 相当＝ここから字下げと同一）。
    if let [Node {
        kind: NodeKind::LineJisage { width },
        ..
    }] = nodes
    {
        return LineRole::BlockOpen(BlockKind::Jisage { width: *width });
    }
    // 規則 6: 行スコープ包み。本文が続く ［＃N字下げ］。
    let jisage_widths = collect_line_jisage(nodes);
    if !jisage_widths.is_empty() {
        // 参照 apply_jisage は見つけるたびバッファ先頭へ unshift するので、
        // **後に書いたものほど外側**になる。外側→内側の順に並べ替える。
        return LineRole::LineWrap(
            jisage_widths
                .into_iter()
                .rev()
                .map(|width| BlockKind::Jisage { width })
                .collect(),
        );
    }
    // 規則 6: 行スコープ地付き／字上げ ［＃地付き］text（先頭が is_block=false の
    // Chitsuki）。参照 renderer は先頭ノードで判定し、行末でブロックを閉じる（1行包み）。
    if let Some(Node {
        kind: NodeKind::BlockStart { block_type, params },
        ..
    }) = nodes.first()
    {
        if !params.is_block && *block_type == BlockType::Chitsuki {
            return LineRole::LineWrap(vec![BlockKind::Chitsuki {
                width: params.width.unwrap_or(0),
            }]);
        }
    }
    // 規則 7: 内容行。
    LineRole::Content
}

/// 行が素の改行（[`NodeKind::UnclosedAccentBreak`]）で終わるか。
///
/// 未閉じ `〔` が行末まで達したときトークナイザが置く。参照実装が
/// `"<br />\r\n"` をバッファへ積むのに当たり、その行は次の行と 1 つの出力単位になる。
fn ends_with_hard_break(nodes: &[Node]) -> bool {
    matches!(
        nodes.last().map(|n| &n.kind),
        Some(NodeKind::UnclosedAccentBreak)
    )
}

/// この行がぶら下げ（折り返し字下げ）を開く行なら、(開始マーカーの位置, 種類) を返す。
fn burasage_open(nodes: &[Node]) -> Option<(usize, BlockKind)> {
    // 同じ行に「終わり」があるなら同行開閉の範囲形。to_inlines がインラインへ畳む。
    if nodes
        .iter()
        .any(|n| matches!(n.kind, NodeKind::BlockEnd { .. }))
    {
        return None;
    }
    let idx = nodes.iter().position(|n| {
        matches!(&n.kind,
            NodeKind::BlockStart { block_type: BlockType::Burasage, params } if params.is_block)
    })?;
    let NodeKind::BlockStart { block_type, params } = &nodes[idx].kind else {
        unreachable!("position で BlockStart を選んでいる")
    };
    block_kind_of(block_type, params).map(|kind| (idx, kind))
}

/// 同じ行に対応する開始が無い `BlockEnd` の位置と `explicit_close` を、現れる順に返す。
///
/// 同じ行で開閉する範囲（`［＃ここから太字］…［＃ここで太字終わり］`）の終端は
/// `to_inlines` がインラインに畳むのでここでは拾わない。拾うのは
/// 前の行から続いているブロックを閉じるものだけ。
fn find_unmatched_block_ends(nodes: &[Node]) -> Vec<(usize, bool)> {
    let mut open: Vec<&BlockType> = Vec::new();
    let mut out = Vec::new();
    for (idx, node) in nodes.iter().enumerate() {
        match &node.kind {
            // 行途中の地付き（is_block=false の Chitsuki）は参照 close_inline_blocks が
            // 行末で閉じるので、同じ行の `［＃ここで地付き終わり］` はこれではなく
            // 前の行から続く複数行の地付きを閉じる。開きとして数えない。
            NodeKind::BlockStart {
                block_type: BlockType::Chitsuki,
                params,
            } if !params.is_block => {}
            NodeKind::BlockStart { block_type, .. } => open.push(block_type),
            NodeKind::BlockEnd {
                block_type,
                params,
                explicit_close,
            } => match open.iter().rposition(|bt| *bt == block_type) {
                Some(pos) => {
                    open.truncate(pos);
                }
                // 複数行ブロックになれない種類（割り注・縦中横など。開始側が注記
                // として描画され BlockStart ノードを作らない）は閉じの対象にしない。
                None if block_kind_of(block_type, params).is_some() => {
                    out.push((idx, *explicit_close))
                }
                None => {}
            },
            _ => {}
        }
    }
    out
}

/// 行単位字下げ `［＃N字下げ］` の幅を**ソース順に**集める。ルビ親文字の中も見る。
///
/// `｜［＃２字下げ］あいう《るび》` のように `｜` の直後に書かれると、
/// トークナイザが親文字（`PrefixedRuby` の base）に取り込んでしまい、
/// トップレベルからは見えなくなる。参照実装の `apply_jisage` はルビの状態に
/// 関わらず `@buffer` へ unshift するので、行全体が字下げ div に包まれる。
pub(super) fn collect_line_jisage(nodes: &[Node]) -> Vec<Option<u32>> {
    let mut out = Vec::new();
    for node in nodes {
        match &node.kind {
            NodeKind::LineJisage { width } => out.push(*width),
            NodeKind::Ruby { children, .. } => out.extend(collect_line_jisage(children)),
            _ => {}
        }
    }
    out
}
