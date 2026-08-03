//! 解決器: 行の事実（[`super::facts`]）から [`LowerPlan`] を組む。
//! `docs/spec-lowerer-constraints.md` の制約 3・4・6・7・8。
//!
//! ここが移行の主戦場である。位置順に走査する [`PlanStack`] は制約 3（対応と包含）を
//! 線形時間で解く実装であって、仕様そのものではない。判断（対応・暗黙閉じ・行結合・
//! `CloseKind`/`Break`・EOF 診断）はすべてここで確定し、[`super::plan::materialize`] は
//! 状態を持たない写像だけを行う。

use super::break_policy::content_break;
use super::facts::{collect_line_jisage, line_facts, LineRole};
use super::inline::to_inlines;
use super::plan::{LowerPlan, PlanBlock, PlanLine};
use super::LowerDiagnostic;

use crate::ast::{BlockKind, Break, CloseKind, Inline, OpenKind};
use crate::node::{BlockType, Node, NodeKind};
use crate::parser::reference_resolver::{resolve_inline_ruby, resolve_references};
use crate::parser::RawDoc;

pub(super) fn solve(raw: &RawDoc) -> LowerPlan {
    let mut stack = PlanStack::new();
    let mut diags: Vec<LowerDiagnostic> = Vec::new();

    // 未閉じ `〔` で次の行を吸収した（＝この添字の行は出力しない）ことを覚える。
    let mut swallowed = std::collections::HashSet::new();
    // 出力を飛ばした行から次の行へ持ち越す内容（参照実装のバッファ持ち越し）。
    let mut carry: Vec<Inline> = Vec::new();

    for (idx, raw_line) in raw.lines.iter().enumerate() {
        if swallowed.contains(&idx) {
            continue;
        }
        let line_no = raw_line.line_no;
        // 前方参照とルビ親文字を解決してから畳む（旧経路と同順）。span は畳み込みに使わない。
        let mut nodes = raw_line.nodes.clone();
        resolve_references(&mut nodes);
        resolve_inline_ruby(&mut nodes);

        let facts = line_facts(&nodes);

        // 出力を飛ばして次の行へ持ち越す 2 つの形。どちらも持ち越しを**確定させない**
        // （未結合断片はこれらの行が続いても累積する。制約 6）ので、下の確定より前に見る。
        match facts.role {
            // 規則 1: ぶら下げを開く行は出力せず、内容を次の行へ持ち越す。
            //
            // 参照 `apply_burasage` は先頭で `@noprint = true` を**無条件に**立て、
            // `general_output` は `@noprint` のときバッファを流さずに return する。
            // その結果、開始タグの前後にあった本文はどちらもバッファに残り、次の行と
            // 1 つの出力単位（＝1 つの per-line ぶら下げ div）になる
            // （`［＃ここから改行天付き、折り返して１字下げ］開始行` ＋ 次行 → 1 つの div。
            // 実文書 001885/58012・001240/46361）。
            LineRole::BurasageOpen { marker, ref kind } => {
                open_block_after_virtual_ends(&mut stack, kind.clone(), line_no, OpenKind::NoBreak);
                carry.extend(to_inlines(&nodes[..marker]));
                carry.extend(to_inlines(&nodes[marker + 1..]));
                continue;
            }
            // 未閉じ `〔` の行も出力せず、行末に素の `<br />` を足して持ち越す
            // （参照 `AccentParser#general_output` が `"<br />\r\n"` を積んで改行ごと食べる）。
            // 行スコープ包み（`［＃地から１字上げ］〔…］`）だけは包みごと持ち越す器が要るので
            // 従来どおり下の `LineWrap` 側で扱う。
            LineRole::Content if facts.hard_break => {
                carry.extend(to_inlines(&nodes));
                continue;
            }
            _ => {}
        }

        // 持ち越しを繋げられるのは内容行だけ。それ以外の行に当たったら、持ち越しを
        // 独立した行として出す（＝マージ前の従来どおりの出力に戻す）。参照はここでも
        // 繋げるが、その組み合わせはコーパスに現れず正しい姿を決められない。
        let carried = if matches!(facts.role, LineRole::Content) {
            std::mem::take(&mut carry)
        } else {
            if !carry.is_empty() {
                let pending = std::mem::take(&mut carry);
                let brk = content_break(&pending, false);
                stack.push_line(pending, brk, line_no);
            }
            Vec::new()
        };

        match facts.role {
            // 上の match で continue 済み。
            LineRole::BurasageOpen { .. } => unreachable!("ぶら下げ開始行は持ち越しへ抜ける"),
            // 規則 2（単独の開始）と規則 6 の例外（`［＃N字下げ］` 1 個だけからなる行）。
            // どちらも制約 4 の仮想終端を伴い、`OpenKind::Newline` で開く。
            LineRole::BlockOpen(kind) => {
                open_block_after_virtual_ends(&mut stack, kind, line_no, OpenKind::Newline);
            }
            LineRole::BlockOpenWithTail(idx, kind) => {
                // 開始タグより前の本文は開くブロックの外に出る。改行は開始タグ以降が
                // 出すので Break::NoNewline。開始タグ直後にも改行は出ない（OpenKind）。
                //
                // 規則 2 と違い制約 4 の仮想終端を挿入しない（open_block_after_virtual_ends
                // を通さない）。理由はそちらの doc コメントを見よ。
                if idx > 0 {
                    stack.push_line(to_inlines(&nodes[..idx]), Break::NoNewline, line_no);
                }
                stack.open_block(kind, line_no, OpenKind::NoBreak);
                let inline = to_inlines(&nodes[idx + 1..]);
                let brk = content_break(&inline, false);
                stack.push_line(inline, brk, line_no);
            }
            LineRole::BlockClose(explicit) => {
                // 対応する開きが無ければ何も出さない（旧経路も未マッチ終了は無出力）。
                // ここは Closes([(0, explicit)]) と同じ処理だが、開きが無いときだけ
                // 扱いが違う（あちらは空の内容行を積む）。開き無しの「終わり」は参照実装が
                // エラーで停止する入力なのでオラクルで是非を決められない。捨てる側を
                // 残しているのは、エディタのプレビューに空行が紛れない方が良いため。
                stack.close_block(|kind, s| block_close_kind(explicit, kind, s));
            }
            LineRole::Closes(closes) => apply_closes(&mut stack, &nodes, &closes, line_no),
            LineRole::LineWrap(kinds) => {
                // ［＃N字下げ］text／行スコープ地付き: 行全体を div で1行に包む。
                // 先頭の行スコープマーカー1個（LineJisage、または is_block=false の
                // 行スコープ BlockStart）だけを取り除き、残りは to_inlines に渡す
                // （行内の見出しコマンド範囲などはそちらが畳む）。
                let rest = strip_line_scope_marker(nodes);
                // 未閉じ `〔` があると参照 AccentParser が改行ごと食べ、`"<br />\r\n"` を
                // 内容の末尾に積んで次の行と 1 つの出力単位にする。閉じタグを持たない行なら
                // 前後どちらに `<br />` が出ても同じバイト列になるが、この行スコープ包みでは
                // 閉じ `</div>` より前に出るので差が出る（60380/60385）。
                //
                // 吸収した次の行が空行のときだけ扱う。中身のある行を本当にこの div の中へ
                // 畳む必要がある入力はオラクルに現れないので、正しい姿を決められない。
                // 末尾の `UnclosedAccentBreak` は行の内容としてそのまま包みの中に入るので、
                // 閉じ `</div>` より前に `<br />` が出る（60380/60385）。次の行が
                // 空行ならそれも吸収する（中身のある行を畳む必要がある入力は
                // オラクルに現れないので、正しい姿を決められない）。
                if facts.hard_break
                    && raw
                        .lines
                        .get(idx + 1)
                        .is_some_and(|next| next.nodes.is_empty())
                {
                    swallowed.insert(idx + 1);
                }
                stack.push(PlanBlock::LineWrap {
                    kinds,
                    inline: to_inlines(&rest),
                    line: line_no,
                });
            }
            LineRole::Content => push_content_line_with(&mut stack, carried, &nodes, line_no),
        }
    }

    // 閉じられていないブロックはそのまま閉じる（旧経路の末尾 pop 相当）。
    // 末尾クローズは行を持たないので `</div>\r\n`（Newline）とする。
    while let Some(block) = stack.pop_open() {
        // EOF まで対応する「終わり」が現れなかった＝閉じ忘れの可能性。診断に記録する
        // （出力は従来どおり末尾クローズ。診断は追加返却のみで Block 出力は不変）。
        diags.push(LowerDiagnostic {
            line: block.line,
            kind: block.kind.clone(),
        });
        stack.push(block.into_nested(CloseKind::Newline));
    }

    LowerPlan {
        roots: stack.roots,
        diagnostics: diags,
    }
}

/// 行スコープ包み（`［＃N字下げ］` と行頭の地付き）を取り出して、包む種類と
/// マーカーを除いたノード列を返す。
///
/// 制約 1 は `［＃ここで…終わり］` を含む行を先に規則 4（`Closes`）で拾うので、
/// 行スコープ包みの判定まで来ない。参照実装は `apply_jisage` が閉じの有無に関わらず
/// バッファへ unshift するだけなので、**同じ行に閉じがあっても包みは効く**
/// （`［＃２字下げ］あいう［＃ここで字下げ終わり］` → `<div class="jisage_2">あいう</div>`）。
/// そのため閉じ行の断片にもこれを当てる。
fn take_line_scope_wrap(nodes: &[Node]) -> (Vec<BlockKind>, Vec<Node>) {
    let widths = collect_line_jisage(nodes);
    if !widths.is_empty() {
        // 後に書いたものほど外側（参照 apply_jisage の unshift）。
        let kinds = widths
            .into_iter()
            .rev()
            .map(|width| BlockKind::Jisage { width })
            .collect();
        let mut rest = nodes.to_vec();
        remove_line_jisage(&mut rest);
        return (kinds, rest);
    }
    if let Some(Node {
        kind: NodeKind::BlockStart { block_type, params },
        ..
    }) = nodes.first()
    {
        if !params.is_block && *block_type == BlockType::Chitsuki {
            return (
                vec![BlockKind::Chitsuki {
                    width: params.width.unwrap_or(0),
                }],
                nodes[1..].to_vec(),
            );
        }
    }
    (Vec::new(), nodes.to_vec())
}

/// 閉じ行の断片を1つ積む。行スコープ包みがあれば [`PlanBlock::LineWrap`] にする。
fn push_close_segment(stack: &mut PlanStack, nodes: &[Node], brk: Break, line_no: usize) {
    let (kinds, rest) = take_line_scope_wrap(nodes);
    if kinds.is_empty() {
        stack.push_line(to_inlines(nodes), brk, line_no);
    } else {
        stack.push(PlanBlock::LineWrap {
            kinds,
            inline: to_inlines(&rest),
            line: line_no,
        });
    }
}

/// 余った終わりのマーカーは `to_inlines` が落とす。
/// 「終わり」を含む行（単独行でないもの）を畳む。
///
/// 参照は行を逐次出力するので、1行に複数の「終わり」があればその順に閉じる
/// （例: `［＃ここで小さな文字終わり］［＃ここで字下げ終わり］`）。各閉じの前の本文は、
/// その時点で開いているブロックの内側に出る。行末の改行を出すのは最後の閉じだけで、
/// 後続本文があるならその行が出す。
///
/// 開いている数より「終わり」が多い行（参照実装はエラーで停止する）は余りを無視する。
fn apply_closes(stack: &mut PlanStack, nodes: &[Node], closes: &[(usize, bool)], line_no: usize) {
    // 閉じられる開きが1つも無ければ閉じタグは出ないので、行をまとめて内容行にする。
    let closable = closes.len().min(stack.depth());
    if closable == 0 {
        push_content_line(stack, nodes, line_no);
        return;
    }
    // 以降は「実際に閉じられる並び」だけを見る。
    let closes = &closes[..closable];
    let last_close = closes.last().expect("closable > 0").0;
    let has_tail = last_close + 1 < nodes.len();
    let mut seg_start = 0usize;

    for (n, (idx, explicit)) in closes.iter().enumerate() {
        // 閉じタグより前の本文。行末の改行は閉じタグ以降が出す。
        let segment = (seg_start < *idx).then_some(&nodes[seg_start..*idx]);
        // 参照は閉じタグを buffer に積む（＝本文の続き）ので、本文は閉じるブロックの
        // 内側に出る。ただしぶら下げだけは閉じで indent_stack から降りてしまい
        // per-line の包みが効かなくなるので、その行の本文はブロックの外に出す。
        let closing_burasage = matches!(stack.innermost(), Some(BlockKind::Burasage(_)));
        let is_last = n + 1 == closes.len();
        // 行末の改行を出すのは最後の閉じだけ。後続本文があるなら `</div>` のみ。
        let close_kind = |kind: &BlockKind, s: &PlanStack| {
            if !is_last || has_tail {
                CloseKind::NoBreak
            } else {
                block_close_kind(*explicit, kind, s)
            }
        };

        if closing_burasage {
            stack.close_block(close_kind);
            if let Some(seg) = segment {
                push_close_segment(stack, seg, Break::NoNewline, line_no);
            }
        } else {
            if let Some(seg) = segment {
                push_close_segment(stack, seg, Break::NoNewline, line_no);
            }
            stack.close_block(close_kind);
        }
        seg_start = *idx + 1;
    }

    // 最後の閉じの後ろに残った本文を同じ行に出す。
    if has_tail {
        let explicit = closes.iter().any(|(_, e)| *e);
        let tail = &nodes[last_close + 1..];
        let brk = content_break(&to_inlines(tail), explicit);
        push_close_segment(stack, tail, brk, line_no);
    }
}

/// 内容行として1行を積む。`［＃ここで…終わり］`（explicit_close=true）を含む行は
/// @terprip=false で行末 `<br />` を抑制する（同行開閉の横組み等・複数行ブロックの閉じ行）。
fn push_content_line(stack: &mut PlanStack, nodes: &[Node], line_no: usize) {
    push_content_line_with(stack, Vec::new(), nodes, line_no)
}

/// [`push_content_line`] の、前の行から持ち越した内容を先頭に足す版。
///
/// 持ち越しの由来は [`solve`] の行マージ（参照実装で
/// 出力を飛ばした行のバッファ）。行末 `<br />` の判定は**繋げたあとの行全体**で行う
/// （参照も 1 つのバッファとして `general_output` に渡すため）。
fn push_content_line_with(
    stack: &mut PlanStack,
    carried: Vec<Inline>,
    nodes: &[Node],
    line_no: usize,
) {
    let has_explicit_close = nodes.iter().any(|n| {
        matches!(
            &n.kind,
            NodeKind::BlockEnd {
                explicit_close: true,
                ..
            }
        )
    });
    let mut inline = carried;
    inline.extend(to_inlines(nodes));
    let brk = content_break(&inline, has_explicit_close);
    stack.push_line(inline, brk, line_no);
}

/// 行単位字下げのマーカーをすべて取り除く（ルビ親文字の中も）。
fn remove_line_jisage(nodes: &mut Vec<Node>) {
    nodes.retain(|n| !matches!(&n.kind, NodeKind::LineJisage { .. }));
    for node in nodes.iter_mut() {
        if let NodeKind::Ruby { children, .. } = &mut node.kind {
            remove_line_jisage(children);
        }
    }
}

/// 行スコープ包みを起こしたマーカー1個を取り除いた残りのノード列を返す。
///
/// ［＃N字下げ］（`LineJisage`）は**行内のどこにあっても**1個、行スコープの
/// `BlockStart`（is_block=false の Jisage/Chitsuki＝地付き）は**先頭にあるとき**だけ
/// 取り除く。行内の見出しコマンド範囲などブロックマーカーはそのまま残す
/// （to_inlines が畳む）。
///
/// 位置だけ返して呼び出し側で飛ばす形にはしない。マーカーをまたぐ範囲コマンド
/// （`［＃ここから太字］…［＃N字下げ］…［＃ここで太字終わり］`）があるので、
/// 列を分割すると `to_inlines` が対を見つけられなくなる。
fn strip_line_scope_marker(nodes: Vec<Node>) -> Vec<Node> {
    // LineJisage は**すべて**落とす（それぞれが 1 枚の div になる。参照 apply_jisage）。
    // ルビ親文字の中に入り込んでいることがあるので、そこも見る。
    if !collect_line_jisage(&nodes).is_empty() {
        let mut rest = nodes;
        remove_line_jisage(&mut rest);
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
/// 参照 explicit_close は @tag_stack から取り出した閉じタグを push_chars で
/// バッファへ積むので、閉じタグが String として残りぶら下げの包みに入る。
/// 字下げ・地付き・ぶら下げ自身は該当しない（それらの閉じは String を残さない）。
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
            // 見出しブロックの閉じ `</a></hN>` も同じ（参照 explicit_close は
            // @tag_stack から取り出した閉じタグを push_chars でバッファへ積むので
            // String が残り、ぶら下げの per-line 包みに入る。実測）。
            | BlockKind::Midashi { .. }
    )
}

fn is_jisage_or_burasage(k: &BlockKind) -> bool {
    matches!(k, BlockKind::Jisage { .. } | BlockKind::Burasage { .. })
}

fn is_chitsuki_or_burasage(k: &BlockKind) -> bool {
    matches!(k, BlockKind::Chitsuki { .. } | BlockKind::Burasage { .. })
}

/// 暗黙閉じ（制約 4）が開始の直前に置く**仮想終端**。参照実装 `close_conflicting_blocks`。
///
/// 開く種類ごとに「どれを閉じるか・どう閉じるか・1 個か連続か」が決まる。表は
/// [`VirtualEnd::before_opening`] にあり、当たらない種類は None を返す。
///
/// 仮想終端は通常の終端と同じ閉じ機構（[`PlanStack::close_block`]）を通る。違いは
/// `CloseKind` がこの表で**先に**確定していることで、制約 7 の優先順（後段の導出）には
/// 掛からない。暗黙閉じを一律 `NoBreak` にしてはいけない（実測）。
struct VirtualEnd {
    /// 閉じる相手か（最も内側の生存ブロックに対して判定する）。
    targets: fn(&BlockKind) -> bool,
    /// 閉じタグの出力形（制約 4 の表で確定済み）。
    close: CloseKind,
    /// 該当する限り閉じ続けるか（false なら 1 個で止める）。
    consecutive: bool,
}

impl VirtualEnd {
    /// 制約 4 の表。
    ///
    /// | 新しい開始 | 強制終端の対象 | 個数 | CloseKind |
    /// |---|---|---|---|
    /// | 字下げ | 最も内側の字下げまたはぶら下げ | 1 | `NoBreak` |
    /// | 地付き | 連続する最内側の地付きまたはぶら下げ | 全て | `NoBreak` |
    /// | ぶら下げ | 連続する最内側の字下げまたはぶら下げ | 全て | `Newline` |
    ///
    /// 閉じタグ直後の改行: 開始タグを即座に出すブロック（Jisage/Chitsuki 等）は
    /// `</div><新開始…>` と同じ出力行に続くので改行なし。Burasage は開始行に
    /// 可視タグを出さない per-line モデルなので、暗黙閉じの `</div>` がその
    /// 開始行の唯一の出力＝行末 `\r\n` が付く。
    fn before_opening(kind: &BlockKind) -> Option<Self> {
        match kind {
            BlockKind::Jisage { .. } => Some(Self {
                targets: is_jisage_or_burasage,
                close: CloseKind::NoBreak,
                consecutive: false,
            }),
            BlockKind::Chitsuki { .. } => Some(Self {
                targets: is_chitsuki_or_burasage,
                close: CloseKind::NoBreak,
                consecutive: true,
            }),
            BlockKind::Burasage { .. } => Some(Self {
                targets: is_jisage_or_burasage,
                close: CloseKind::Newline,
                consecutive: true,
            }),
            _ => None,
        }
    }

    /// 仮想終端を通常の閉じ機構へ流す。
    fn close_targets(&self, stack: &mut PlanStack) {
        while stack.innermost().is_some_and(self.targets) {
            stack.close_block(|_, _| self.close);
            if !self.consecutive {
                break;
            }
        }
    }
}

/// 複数行ブロックを開く（制約 3）。開始の直前に制約 4 の仮想終端を挿入してから開く。
///
/// 通すのは制約 1 の規則 1（ぶら下げ開始行）・規則 2（単独の開始）・規則 6 の例外
/// （`［＃N字下げ］` 1 個だけからなる行）だけである。規則 5（行途中の開始）は
/// [`PlanStack::open_block`] を直に呼ぶ——行の途中で暗黙閉じを伴う種類を開く入力は
/// 参照実装がエラーで停止するため（実測）オラクルには現れず、正しい振る舞いを
/// 決められない。そこでは単に入れ子にする。
fn open_block_after_virtual_ends(
    stack: &mut PlanStack,
    kind: BlockKind,
    line_no: usize,
    open: OpenKind,
) {
    if let Some(end) = VirtualEnd::before_opening(&kind) {
        end.close_targets(stack);
    }
    stack.open_block(kind, line_no, open);
}

/// 行末で閉じるブロックの閉じタグの出力形。
///
/// `ここで…終わり`（explicit）は `</div>\r\n`。bare `…終わり` は @terprip 維持で
/// `</div><br />\r\n`（memory bare-block-end）。
///
/// ぶら下げの直下で装飾系ブロックが閉じる行は、参照が閉じタグを String 扱いして
/// per-line の burasage div で包む。包む幅は外側のぶら下げが持つので、ここで畳んで
/// 木に載せる（描画器は状態を持たない）。
fn block_close_kind(explicit: bool, kind: &BlockKind, stack: &PlanStack) -> CloseKind {
    if is_burasage_wrapped_close(kind) {
        if let Some(BlockKind::Burasage(geometry)) = stack.innermost() {
            return CloseKind::BurasageWrapped(*geometry);
        }
    }
    if explicit {
        CloseKind::Newline
    } else {
        CloseKind::BareBreak
    }
}

/// 開いている途中の [`PlanBlock::Nested`]（子ブロックを溜めているビルダー）。
struct OpenPlanBlock {
    kind: BlockKind,
    children: Vec<PlanBlock>,
    /// このブロックを開いた本文行（0 起点）。
    line: usize,
    open: OpenKind,
}

impl OpenPlanBlock {
    /// 閉じ方を決めて [`PlanBlock::Nested`] にする。
    fn into_nested(self, close: CloseKind) -> PlanBlock {
        PlanBlock::Nested {
            kind: self.kind,
            open: self.open,
            close,
            opened_at: self.line,
            children: self.children,
        }
    }
}

/// 解決中のブロック森。開いているブロックのスタックと、まだどのブロックにも属さない
/// トップレベル列を持つ。ブロックを積む・開く・閉じるはすべてここを通す。
///
/// 制約 3（複数行ブロックの対応と包含）を線形時間で解く実装。「最も内側の生存ブロック」は
/// スタックの最上位である。
struct PlanStack {
    open: Vec<OpenPlanBlock>,
    roots: Vec<PlanBlock>,
}

impl PlanStack {
    fn new() -> Self {
        Self {
            open: Vec::new(),
            roots: Vec::new(),
        }
    }

    /// 開いているブロックの数。
    fn depth(&self) -> usize {
        self.open.len()
    }

    /// 今いちばん内側で開いているブロックの種類。
    fn innermost(&self) -> Option<&BlockKind> {
        self.open.last().map(|b| &b.kind)
    }

    /// いちばん内側の開いているブロックへ、無ければトップレベルへ積む。
    fn push(&mut self, block: PlanBlock) {
        match self.open.last_mut() {
            Some(b) => b.children.push(block),
            None => self.roots.push(block),
        }
    }

    /// 内容の1行を積む。
    fn push_line(&mut self, inline: Vec<Inline>, brk: Break, line: usize) {
        self.push(PlanBlock::Content(PlanLine {
            fragments: inline,
            brk,
            line,
        }));
    }

    /// ブロックを開く。
    fn open_block(&mut self, kind: BlockKind, line: usize, open: OpenKind) {
        self.open.push(OpenPlanBlock {
            kind,
            children: Vec::new(),
            line,
            open,
        });
    }

    /// いちばん内側のブロックを閉じて木に載せる。閉じ方は**ポップ後の**スタックから
    /// 決める（ぶら下げ直下かの判定に外側が要る）。開いていなければ何もしない。
    fn close_block(&mut self, close: impl FnOnce(&BlockKind, &Self) -> CloseKind) {
        let Some(block) = self.open.pop() else {
            return;
        };
        let close = close(&block.kind, self);
        self.push(block.into_nested(close));
    }

    /// 閉じられないまま残ったブロックを内側から順に取り出す（EOF 処理用）。
    fn pop_open(&mut self) -> Option<OpenPlanBlock> {
        self.open.pop()
    }
}
