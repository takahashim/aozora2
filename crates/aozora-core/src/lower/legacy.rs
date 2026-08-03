//! **並走検証の基準。編集禁止（削除のみ）。**
//!
//! 制約解消型 Lowerer への移行（`docs/plan-lowerer-migration.md`）のあいだだけ存在する、
//! 移行開始時点の [`super::lower_to_blocks_with_diagnostics`] の逐語コピー。既定経路を
//! 段階変形するあいだ、出力が 1 バイトも変わらないことをこのコピーとの比較で確かめる。
//!
//! **このファイルを編集してはならない。** 基準が変形対象とコードを共有すると、両側が
//! 同時に変わったとき並走が盲目になる。共有してよいのは移行中に触らない凍結境界の葉
//! （`inline::to_inlines` / `break_policy::content_break` / `block_kind_of`）だけで、
//! 移行中にその葉を触る必要が出たら、**先に該当関数をこのファイルへコピーしてから**触る。
//!
//! 移行完了（計画の C6）でファイルごと削除する。

use super::break_policy::content_break;
use super::inline::to_inlines;
use super::{block_kind_of, LowerDiagnostic};

use crate::ast::{AozoraAst, Block, BlockKind, Break, CloseKind, Inline, OpenKind};
use crate::node::{BlockType, Node, NodeKind};
use crate::parser::reference_resolver::{resolve_inline_ruby, resolve_references};
use crate::parser::RawDoc;

/// 移行開始時点の [`super::lower_to_blocks_with_diagnostics`]。並走検証専用。
///
/// `#[doc(hidden)]` の pub なので、統合テストとクレート跨ぎの example から呼べる
/// （`#[cfg(test)]` の `pub(crate)` は example から見えず、`#[path]` include は
/// テストクレート側で `crate::ast` が解決できない）。移行完了で削除する。
#[doc(hidden)]
pub fn lower_to_blocks_legacy(raw: &RawDoc) -> (AozoraAst, Vec<LowerDiagnostic>) {
    let mut stack = BlockStack::new();
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

        let kind = classify_line(&nodes);

        // ぶら下げを開く行は出力せず、内容を次の行へ持ち越す。
        //
        // 参照 `apply_burasage` は先頭で `@noprint = true` を**無条件に**立て、
        // `general_output` は `@noprint` のときバッファを流さずに return する。
        // その結果、開始タグの前後にあった本文はどちらもバッファに残り、次の行と
        // 1 つの出力単位（＝1 つの per-line ぶら下げ div）になる
        // （`［＃ここから改行天付き、折り返して１字下げ］開始行` ＋ 次行 → 1 つの div。
        // 実文書 001885/58012・001240/46361）。
        if let Some((marker, block_kind)) = burasage_open(&nodes) {
            if let Some(policy) = ImplicitClose::when_opening(&block_kind) {
                policy.apply(&mut stack);
            }
            stack.open_block(block_kind, line_no, OpenKind::NoBreak);
            carry.extend(to_inlines(&nodes[..marker]));
            carry.extend(to_inlines(&nodes[marker + 1..]));
            continue;
        }

        // 未閉じ `〔` の行も出力せず、行末に素の `<br />` を足して持ち越す
        // （参照 `AccentParser#general_output` が `"<br />\r\n"` を積んで改行ごと食べる）。
        // 行スコープ包み（`［＃地から１字上げ］〔…］`）だけは包みごと持ち越す器が要るので
        // 従来どおり下の `LineWrap` 側で扱う。
        if ends_with_hard_break(&nodes) && matches!(kind, LineKind::Content) {
            carry.extend(to_inlines(&nodes));
            continue;
        }

        // 持ち越しを繋げられるのは内容行だけ。それ以外の行に当たったら、持ち越しを
        // 独立した行として出す（＝マージ前の従来どおりの出力に戻す）。参照はここでも
        // 繋げるが、その組み合わせはコーパスに現れず正しい姿を決められない。
        let carried = if matches!(kind, LineKind::Content) {
            std::mem::take(&mut carry)
        } else {
            if !carry.is_empty() {
                let pending = std::mem::take(&mut carry);
                let brk = content_break(&pending, false);
                stack.push_line(pending, brk, line_no);
            }
            Vec::new()
        };

        match kind {
            LineKind::BlockOpen(kind) => {
                if let Some(policy) = ImplicitClose::when_opening(&kind) {
                    policy.apply(&mut stack);
                }
                stack.open_block(kind, line_no, OpenKind::Newline);
            }
            LineKind::BlockOpenWithTail(idx, kind) => {
                // 開始タグより前の本文は開くブロックの外に出る。改行は開始タグ以降が
                // 出すので Break::NoNewline。開始タグ直後にも改行は出ない（OpenKind）。
                //
                // BlockOpen と違い implicit_close は行わない。暗黙閉じを伴う種類
                // （Jisage/Chitsuki/Burasage）を行の途中で開く入力は参照実装が
                // エラーで停止するため（実測）オラクルには現れず、正しい振る舞いを
                // 決められない。ここでは単に開いておく。
                if idx > 0 {
                    stack.push_line(to_inlines(&nodes[..idx]), Break::NoNewline, line_no);
                }
                stack.open_block(kind, line_no, OpenKind::NoBreak);
                let inline = to_inlines(&nodes[idx + 1..]);
                let brk = content_break(&inline, false);
                stack.push_line(inline, brk, line_no);
            }
            LineKind::BlockClose(explicit) => {
                // 対応する開きが無ければ何も出さない（旧経路も未マッチ終了は無出力）。
                // ここは Closes([(0, explicit)]) と同じ処理だが、開きが無いときだけ
                // 扱いが違う（あちらは空の内容行を積む）。開き無しの「終わり」は参照実装が
                // エラーで停止する入力なのでオラクルで是非を決められない。捨てる側を
                // 残しているのは、エディタのプレビューに空行が紛れない方が良いため。
                stack.close_block(|kind, s| block_close_kind(explicit, kind, s));
            }
            LineKind::Closes(closes) => apply_closes(&mut stack, &nodes, &closes, line_no),
            LineKind::LineWrap(kinds) => {
                // ［＃N字下げ］text／行スコープ地付き: 行全体を div で1行に包む。
                // 先頭の行スコープマーカー1個（LineJisage、または is_block=false の
                // 行スコープ BlockStart）だけを取り除き、残りは to_inlines に渡す
                // （行内の見出しコマンド範囲などはそちらが畳む）。
                // 吸収の判定に使うので、包みを剥がす前に見ておく。
                let hard_break = ends_with_hard_break(&nodes);
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
                if hard_break
                    && raw
                        .lines
                        .get(idx + 1)
                        .is_some_and(|next| next.nodes.is_empty())
                {
                    swallowed.insert(idx + 1);
                }
                stack.push(Block::LineWrap {
                    kinds,
                    inline: to_inlines(&rest),
                    line: line_no,
                });
            }
            LineKind::Content => push_content_line_with(&mut stack, carried, &nodes, line_no),
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

    (stack.top, diags)
}

/// 行スコープ包み（`［＃N字下げ］` と行頭の地付き）を取り出して、包む種類と
/// マーカーを除いたノード列を返す。
///
/// `classify_line` は `［＃ここで…終わり］` を含む行を先に `Closes` として扱うので、
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

/// 閉じ行の断片を1つ積む。行スコープ包みがあれば [`Block::LineWrap`] にする。
fn push_close_segment(stack: &mut BlockStack, nodes: &[Node], brk: Break, line_no: usize) {
    let (kinds, rest) = take_line_scope_wrap(nodes);
    if kinds.is_empty() {
        stack.push_line(to_inlines(nodes), brk, line_no);
    } else {
        stack.push(Block::LineWrap {
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
fn apply_closes(stack: &mut BlockStack, nodes: &[Node], closes: &[(usize, bool)], line_no: usize) {
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
        let close_kind = |kind: &BlockKind, s: &BlockStack| {
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
fn push_content_line(stack: &mut BlockStack, nodes: &[Node], line_no: usize) {
    push_content_line_with(stack, Vec::new(), nodes, line_no)
}

/// [`push_content_line`] の、前の行から持ち越した内容を先頭に足す版。
///
/// 持ち越しの由来は [`lower_to_blocks_legacy`] の行マージ（参照実装で
/// 出力を飛ばした行のバッファ）。行末 `<br />` の判定は**繋げたあとの行全体**で行う
/// （参照も 1 つのバッファとして `general_output` に渡すため）。
fn push_content_line_with(
    stack: &mut BlockStack,
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

/// 行単位字下げ `［＃N字下げ］` の幅を**ソース順に**集める。ルビ親文字の中も見る。
///
/// `｜［＃２字下げ］あいう《るび》` のように `｜` の直後に書かれると、
/// トークナイザが親文字（`PrefixedRuby` の base）に取り込んでしまい、
/// トップレベルからは見えなくなる。参照実装の `apply_jisage` はルビの状態に
/// 関わらず `@buffer` へ unshift するので、行全体が字下げ div に包まれる。
fn collect_line_jisage(nodes: &[Node]) -> Vec<Option<u32>> {
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

/// ブロックを開くときに暗黙で閉じる相手（参照実装 `close_conflicting_blocks`）。
///
/// 開く種類ごとに「どれを閉じるか・どう閉じるか・1つだけか」が決まる。
/// 暗黙閉じを持たない種類では [`ImplicitClose::when_opening`] が None を返す。
struct ImplicitClose {
    /// 閉じる相手か（スタック最上位に対して判定する）。
    matches: fn(&BlockKind) -> bool,
    /// 暗黙閉じの閉じタグの出力形。
    close: CloseKind,
    /// 1つ閉じたら止めるか（false なら該当する限り閉じ続ける）。
    once: bool,
}

impl ImplicitClose {
    /// 閉じタグ直後の改行: 開始タグを即座に出すブロック（Jisage/Chitsuki 等）は
    /// `</div><新開始…>` と同じ出力行に続くので改行なし。Burasage は開始行に
    /// 可視タグを出さない per-line モデルなので、暗黙閉じの `</div>` がその
    /// 開始行の唯一の出力＝行末 `\r\n` が付く。
    fn when_opening(kind: &BlockKind) -> Option<Self> {
        match kind {
            // Jisage 開始: 最上位が Jisage/Burasage なら1つだけ閉じる。
            BlockKind::Jisage { .. } => Some(Self {
                matches: is_jisage_or_burasage,
                close: CloseKind::NoBreak,
                once: true,
            }),
            // Chitsuki 開始: 最上位から Chitsuki/Burasage が続く限り閉じる。
            BlockKind::Chitsuki { .. } => Some(Self {
                matches: is_chitsuki_or_burasage,
                close: CloseKind::NoBreak,
                once: false,
            }),
            // Burasage 開始: 最上位から Jisage/Burasage が続く限り閉じる。
            BlockKind::Burasage { .. } => Some(Self {
                matches: is_jisage_or_burasage,
                close: CloseKind::Newline,
                once: false,
            }),
            _ => None,
        }
    }

    fn apply(&self, stack: &mut BlockStack) {
        while stack.innermost().is_some_and(self.matches) {
            stack.close_block(|_, _| self.close);
            if self.once {
                break;
            }
        }
    }
}

/// 行末で閉じるブロックの閉じタグの出力形。
///
/// `ここで…終わり`（explicit）は `</div>\r\n`。bare `…終わり` は @terprip 維持で
/// `</div><br />\r\n`（memory bare-block-end）。
///
/// ぶら下げの直下で装飾系ブロックが閉じる行は、参照が閉じタグを String 扱いして
/// per-line の burasage div で包む。包む幅は外側のぶら下げが持つので、ここで畳んで
/// 木に載せる（描画器は状態を持たない）。
fn block_close_kind(explicit: bool, kind: &BlockKind, stack: &BlockStack) -> CloseKind {
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

/// 開いている途中の [`Block::Nested`]（子ブロックを溜めているビルダー）。
struct OpenBlock {
    kind: BlockKind,
    children: Vec<Block>,
    /// このブロックを開いた本文行（0 起点）。
    line: usize,
    open: OpenKind,
}

impl OpenBlock {
    /// 閉じ方を決めて [`Block::Nested`] にする。
    fn into_nested(self, close: CloseKind) -> Block {
        Block::Nested {
            kind: self.kind,
            children: self.children,
            close,
            open: self.open,
            line: self.line,
        }
    }
}

/// 畳み込み中のブロック木。開いているブロックのスタックと、まだどのブロックにも
/// 属さないトップレベル列を持つ。ブロックを積む・開く・閉じるはすべてここを通す。
struct BlockStack {
    open: Vec<OpenBlock>,
    top: AozoraAst,
}

impl BlockStack {
    fn new() -> Self {
        Self {
            open: Vec::new(),
            top: Vec::new(),
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
    fn push(&mut self, block: Block) {
        match self.open.last_mut() {
            Some(b) => b.children.push(block),
            None => self.top.push(block),
        }
    }

    /// 内容の1行を積む。
    fn push_line(&mut self, inline: Vec<Inline>, brk: Break, line: usize) {
        self.push(Block::Line { inline, brk, line });
    }

    /// ブロックを開く。
    fn open_block(&mut self, kind: BlockKind, line: usize, open: OpenKind) {
        self.open.push(OpenBlock {
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
    fn pop_open(&mut self) -> Option<OpenBlock> {
        self.open.pop()
    }
}

/// 行の種類。
enum LineKind {
    /// ブロック開始（`ここから…`）。単独行の BlockStart(is_block=true)。
    BlockOpen(BlockKind),
    /// ブロック終了。単独行の BlockEnd。bool は explicit_close（`ここで…終わり`=true、
    /// bare `…終わり`=false）。
    BlockClose(bool),
    /// 単独行でない「終わり」を含む行。要素は (BlockEnd の位置, explicit_close) で、
    /// 現れる順に並ぶ。参照実装は行を逐次出力するので、各閉じの前の本文はその時点で
    /// 開いているブロックの内側に出し、最後の閉じの後ろの本文は同じ行に続ける。
    Closes(Vec<(usize, bool)>),
    /// 行の途中で開く複数行ブロック（`text［＃ここから斜体］text` / 行頭で開いて
    /// 本文が続く `［＃ここからキャプション］text`）。usize は BlockStart の位置。
    /// 参照は開始タグをその場に出し、同じ行に内容を続ける。
    BlockOpenWithTail(usize, BlockKind),
    /// 行スコープの1行包み（同行に本文あり）。字下げ／地付き。
    /// 行を包むブロック（外側→内側の順）。`［＃N字下げ］` は1行に複数書ける。
    LineWrap(Vec<BlockKind>),
    /// 内容行。
    Content,
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
///
/// 参照 `apply_burasage` は開始タグの位置に関係なく `@noprint` を立てるので、
/// 単独行（[`LineKind::BlockOpen`]、マーカー位置 0）でも同行に本文が続く形
/// （[`LineKind::BlockOpenWithTail`]）でも同じ扱いになる。
/// [`classify_line`] を経由せずノード列から直接見るのは、`classify_line` の
/// [`LineKind::BlockOpenWithTail`] が「マーカーの**後ろ**に要素がある」ことを求めるため。
/// 参照はマーカーの位置を問わず `apply_burasage` を実行するので、行末にマーカーがある
/// `\n［＃ここから改行天付き、折り返して１字下げ］`（本文中の裸 LF）も開く
/// （実文書 001240/46361）。
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
/// [`to_inlines`] がインラインに畳むのでここでは拾わない。拾うのは
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

/// 解決済みノード列から行の種類を判定する。
///
/// **判定順そのものが仕様**なので、上から順に:
///
/// 1. 単独の `BlockStart`(is_block=true) → ブロック開始
/// 2. 単独の `BlockEnd` → ブロック終了
/// 3. 同じ行に対応する開始が無い `BlockEnd` → 行途中クローズ（[`LineKind::Closes`]）。
///    開始より先に見るのは、閉じてから開く行で閉じを落とさないため
/// 4. 行内に `BlockEnd` が無い `BlockStart`(is_block=true) → 行途中オープン。
///    同じ行で開閉が揃う範囲形は `to_inlines` が `BlockInline` に畳むので除く
/// 5. `LineJisage` 単独 → ブロック開始 / 行内にあれば行包み
/// 6. 行頭の行スコープ地付き → 行包み
/// 7. それ以外 → 内容行
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
            return LineKind::BlockClose(*explicit_close);
        }
    }
    // 「終わり」を含む行（単独行は上で処理済み）。同じ行で開閉する範囲形は
    // `to_inlines` がインラインに畳むので、対応する開始が同じ行に無いものだけを拾う。
    let closes = find_unmatched_block_ends(nodes);
    if !closes.is_empty() {
        return LineKind::Closes(closes);
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
        // 「同じ行で閉じているか」を、種類を問わない BlockEnd の有無で見る
        // （inline.rs の find_matching_end は同種で対応を取る）。両者が食い違うのは
        // 別種の終わりが混ざる行（`text［＃ここから斜体］text［＃ここで太字終わり］`）
        // だけで、これは参照実装がエラーで停止する入力なので正解を決められない。
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
    // 行単位字下げ ［＃N字下げ］。行にこのマーカーしか無ければ複数行ブロックを開く
    // （参照 apply_jisage の unshift 相当＝ここから字下げと同一）。本文が続けば行包み。
    if let [Node {
        kind: NodeKind::LineJisage { width },
        ..
    }] = nodes
    {
        return LineKind::BlockOpen(BlockKind::Jisage { width: *width });
    }
    let jisage_widths = collect_line_jisage(nodes);
    if !jisage_widths.is_empty() {
        // 参照 apply_jisage は見つけるたびバッファ先頭へ unshift するので、
        // **後に書いたものほど外側**になる。外側→内側の順に並べ替える。
        return LineKind::LineWrap(
            jisage_widths
                .into_iter()
                .rev()
                .map(|width| BlockKind::Jisage { width })
                .collect(),
        );
    }
    // 行スコープ地付き／字上げ ［＃地付き］text（先頭が is_block=false の Chitsuki）。
    // 参照 renderer は先頭ノードで判定し、行末でブロックを閉じる（1行包み）。
    if let Some(Node {
        kind: NodeKind::BlockStart { block_type, params },
        ..
    }) = nodes.first()
    {
        if !params.is_block && *block_type == BlockType::Chitsuki {
            return LineKind::LineWrap(vec![BlockKind::Chitsuki {
                width: params.width.unwrap_or(0),
            }]);
        }
    }
    LineKind::Content
}
