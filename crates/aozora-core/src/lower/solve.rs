//! 解決器: 行の事実（[`super::facts`]）から [`LowerPlan`] を組む。
//! `docs/spec-lowerer-constraints.md` の制約 3・4・6・7・8。
//!
//! ここが移行の主戦場である。位置順に走査する [`PlanStack`] は制約 3（対応と包含）を
//! 線形時間で解く実装であって、仕様そのものではない。判断（対応・暗黙閉じ・行結合・
//! `CloseKind`/`Break`・EOF 診断）はすべてここで確定し、[`super::plan::materialize`] は
//! 状態を持たない写像だけを行う。

use super::break_policy::content_break;
use super::facts::{collect_line_jisage, line_facts, CloseFact, LineRole};
use super::inline::to_inlines;
use super::plan::{check_plan_invariants, LowerPlan, PlanBlock, PlanLine};
use super::{LowerDiagnostic, LowerDiagnosticKind};

use crate::ast::{BlockKind, Break, CloseKind, Inline, OpenKind};
use crate::node::{BlockType, Node, NodeKind};
use crate::parser::reference_resolver::{resolve_inline_ruby, resolve_references};
use crate::parser::RawDoc;

pub(super) fn solve(raw: &RawDoc) -> LowerPlan {
    let mut stack = PlanStack::new();
    let mut diags: Vec<LowerDiagnostic> = Vec::new();

    // 行スコープ包みの未閉じ `〔` が吸収した行（制約 6 の `suppressed_by`）。
    // 走査を飛ばすのは `raw.lines` の**添字**で、Plan に載せるのは**行番号**である。
    // 節ごとに畳む経路（`interchange::RawDocument::to_aozora`）では文書全体で採番した
    // 行番号を持つ行が節の途中から始まるので、両者は一致しない。
    let mut suppressed = std::collections::HashSet::new();
    let mut suppressed_lines: Vec<usize> = Vec::new();
    // 結合待ちの断片（制約 6）。
    let mut joins = Joins::default();

    for (idx, raw_line) in raw.lines.iter().enumerate() {
        if suppressed.contains(&idx) {
            continue;
        }
        let line_no = raw_line.line_no;
        // 前方参照とルビ親文字を解決してから畳む（旧経路と同順）。span は畳み込みに使わない。
        let mut nodes = raw_line.nodes.clone();
        resolve_references(&mut nodes);
        resolve_inline_ruby(&mut nodes);

        let facts = line_facts(&nodes);

        // 出力を飛ばして後続へ結合する 2 つの形。どちらも未結合断片を**確定させない**
        // （これらの行が続いても累積する。制約 6）ので、下の確定より前に見る。
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
                // 開始マーカーの前後を別々に to_inlines へ渡す。連結してから 1 回で
                // 呼ぶと、同一行範囲（制約 2）の対応づけが変わりうる。
                joins.defer(line_no, to_inlines(&nodes[..marker]));
                joins.defer(line_no, to_inlines(&nodes[marker + 1..]));
                continue;
            }
            // 未閉じ `〔` の行も出力せず、行末に素の `<br />` を足して持ち越す
            // （参照 `AccentParser#general_output` が `"<br />\r\n"` を積んで改行ごと食べる）。
            // 行スコープ包み（`［＃地から１字上げ］〔…］`）だけは包みごと持ち越す器が要るので
            // 従来どおり下の `LineWrap` 側で扱う。
            LineRole::Content if facts.hard_break => {
                joins.defer(line_no, to_inlines(&nodes));
                continue;
            }
            _ => {}
        }

        // 断片の結合先になれるのは内容行だけ。それ以外の行に当たったら、未結合断片を
        // 独立した行として確定する（＝マージ前の従来どおりの出力に戻す）。参照はここでも
        // 繋げるが、その組み合わせはコーパスに現れず正しい姿を決められない。
        let carried = if matches!(facts.role, LineRole::Content) {
            joins.attach()
        } else {
            joins.settle(&mut stack, line_no);
            Carried::default()
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
                // マーカーの前に空白でない内容があれば診断に出す（木は変えない）。
                if head_has_content(&nodes[..idx]) {
                    diags.push(LowerDiagnostic {
                        line: line_no,
                        kind: LowerDiagnosticKind::MidlineBlockOpen(kind.clone()),
                    });
                }
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
            LineRole::BlockClose(close) => {
                // 制約 3: 最内層と種類が一致するときだけ閉じる。一致しなければ何も出さず
                // 診断に出す（参照 check_close_match は停止する）。閉じない側で空の内容行を
                // 積まないのは、エディタのプレビューに空行が紛れない方が良いため。
                match stack.innermost() {
                    Some(innermost) if end_matches(&close.written, innermost) => {
                        stack.close_block(Closure::End {
                            explicit: close.explicit,
                            followed_by_content: false,
                        });
                    }
                    innermost => diags.push(LowerDiagnostic {
                        line: line_no,
                        kind: LowerDiagnosticKind::UnmatchedEnd {
                            written: close.written,
                            innermost: innermost.cloned(),
                        },
                    }),
                }
            }
            LineRole::Closes(closes) => {
                apply_closes(&mut stack, &nodes, &closes, line_no, &mut diags)
            }
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
                if let Some(next) = raw
                    .lines
                    .get(idx + 1)
                    .filter(|next| facts.hard_break && next.nodes.is_empty())
                {
                    suppressed.insert(idx + 1);
                    suppressed_lines.push(next.line_no);
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

    // EOF まで生存した開始には仮想終端を置いて閉じる（制約 8）。診断は内側から順。
    while let Some(block) = stack.pop_open() {
        // EOF まで対応する「終わり」が現れなかった＝閉じ忘れの可能性。診断に記録する
        // （出力は従来どおり末尾クローズ。診断は追加返却のみで Block 出力は不変）。
        diags.push(LowerDiagnostic {
            line: block.line,
            kind: LowerDiagnosticKind::UnclosedBlock(block.kind.clone()),
        });
        let close = close_kind(&Closure::Eof, &block.kind, stack.innermost());
        stack.push(block.into_nested(close));
    }

    let plan = LowerPlan {
        roots: stack.roots,
        suppressed_lines,
        diagnostics: diags,
    };
    if cfg!(debug_assertions) {
        check_plan_invariants(&plan);
    }
    plan
}

/// 行結合（制約 6）。出力を飛ばした行の内容断片を、結合先が現れるまで持つ。
///
/// 参照実装は出力を飛ばした行のバッファを次の行へ持ち越す。設計上これは可変の
/// 持ち越しではなく断片から断片への `joins_before` 関係で、その連結成分がひとつの
/// [`PlanBlock::Content`] になる。走査が位置順の単一パスなので、同時に育つ連結成分は
/// 高々ひとつ——ここではそれを [`Joins::chain`] として持つ。断片に id を振ってグラフに
/// しても得られる答えは同じで、読む手間だけが増える。
#[derive(Default)]
struct Joins {
    /// 育っている連結成分（結合順に連結済み）。
    chain: Vec<Inline>,
    /// 連結成分に断片を出した行（昇順・重複なし）。
    source_lines: Vec<usize>,
}

/// 連結成分をひとつ受け取った結果（結合先の行が使う）。
#[derive(Default)]
struct Carried {
    fragments: Vec<Inline>,
    source_lines: Vec<usize>,
}

impl Joins {
    /// 断片を後続へ結合する（この行は出力しない）。
    ///
    /// 同じ行から複数回呼ばれることがある（ぶら下げ開始行はマーカーの前後を別々に
    /// 渡す）ので、由来の行は重複させずに 1 度だけ記録する。
    fn defer(&mut self, line_no: usize, fragments: Vec<Inline>) {
        if self.source_lines.last() != Some(&line_no) {
            self.source_lines.push(line_no);
        }
        self.chain.extend(fragments);
    }

    /// 結合先の内容行が来た。連結成分を渡して空にする。
    ///
    /// 行末 `<br />` の判定は**繋げたあとの行全体**で行う（参照も 1 つのバッファとして
    /// `general_output` に渡す）ので、ここでは判定せずに断片だけを返す。
    fn attach(&mut self) -> Carried {
        Carried {
            fragments: std::mem::take(&mut self.chain),
            source_lines: std::mem::take(&mut self.source_lines),
        }
    }

    /// 結合先が来ないまま終わった断片を、独立した行として確定する。
    ///
    /// 確定するのは、次に来た行が内容行でも結合元でもないときだけである（ぶら下げ
    /// 開始行や `HardBreak` 行が続くあいだは累積する。実測: ぶら下げ開始行が 3 行
    /// 続けば 3 行が 1 つの断片列になる）。
    ///
    /// 行番号は**確定を起こした行**のものにする（結合元の行番号ではない。現行の quirk）。
    /// 確定した断片は明示終端を含まないものとして扱う（制約 7）。
    fn settle(&mut self, stack: &mut PlanStack, line_no: usize) {
        if self.chain.is_empty() {
            return;
        }
        let carried = self.attach();
        let brk = content_break(&carried.fragments, false);
        stack.push(PlanBlock::Content(PlanLine {
            fragments: carried.fragments,
            brk,
            line: line_no,
            source_lines: carried.source_lines,
        }));
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

/// 行途中の開始マーカーの**前**に、空白でない内容があるか。
///
/// 注記一覧に行中の `ここから…` の例は無い（全例・記入例とも 46 箇所すべて行頭。実測）。
/// 一方、開始タグの直後から本文が始まる形（前が空か空白だけ）は実文書にある
/// （001065/18361 の `　［＃ここから斜体］Fourscore…`、001841/57318 の
/// `［＃ここからキャプション］図３　ペラグラ患者。`）。前者だけを診断に出すための判定で、
/// 全角空白は `char::is_whitespace` が真を返す（U+3000 は White_Space）。
fn head_has_content(nodes: &[Node]) -> bool {
    nodes.iter().any(|node| match &node.kind {
        NodeKind::Text(text) => !text.chars().all(char::is_whitespace),
        _ => true,
    })
}

/// 記法に書かれた種類が、開いているブロックを閉じられるか（**制約 3 の最内層照合**）。
///
/// 参照 `check_close_match` は `detect_command_mode` が返す `INDENT_TYPE` のキーと
/// `@indent_stack.last` を比べ、違えば「〈種類〉を閉じようとしましたが、〈種類〉中では
/// ありません」で停止する（実測）。照合するのは**最内層だけ**である。「一致する最も
/// 内側のブロック」を探しにいく形にすると内側の異種を飛び越して外側を閉じることになり、
/// 制約 3 の「区間は交差せず、入れ子か互いに素」が破れる。
///
/// 2 つの細部も参照から取った（実測）:
/// - ぶら下げ中は `hanging_indent?` が `:jisage` を返すので、**ぶら下げは字下げの終端で
///   閉じる**（`［＃ここで字下げ終わり］`）。
/// - 大きな文字と小さな文字は別のキー（`dai`/`sho`）なので、段階の大小違いは一致しない。
///
/// 複数行ブロックになれない種類（縦中横・割り注・割書・装飾・注記付き範囲）は、一致する
/// 開きが存在しえないので常に false になる。規則 3 と規則 4 の非対称は、別途ガードを
/// 書かなくてもこの帰結として消える。
///
/// **`written` 側に `_` の catch-all を置かないこと。** [`BlockType`] に variant を
/// 足したとき、ここが黙って「閉じない」を選ぶのを防ぐ。
fn end_matches(written: &BlockType, open: &BlockKind) -> bool {
    use crate::node::FontSizeType;
    match written {
        BlockType::Jisage => {
            matches!(open, BlockKind::Jisage { .. } | BlockKind::Burasage(_))
        }
        BlockType::Burasage => matches!(open, BlockKind::Burasage(_)),
        BlockType::Chitsuki => matches!(open, BlockKind::Chitsuki { .. }),
        BlockType::Jizume => matches!(open, BlockKind::Jizume { .. }),
        BlockType::Keigakomi => matches!(open, BlockKind::Keigakomi),
        BlockType::Midashi => matches!(open, BlockKind::Midashi { .. }),
        BlockType::Yokogumi => matches!(open, BlockKind::Yokogumi),
        BlockType::Caption => matches!(open, BlockKind::Caption),
        BlockType::Futoji => matches!(open, BlockKind::Futoji),
        BlockType::Shatai => matches!(open, BlockKind::Shatai),
        BlockType::FontDai => matches!(
            open,
            BlockKind::FontSize {
                size_type: FontSizeType::Dai,
                ..
            }
        ),
        BlockType::FontSho => matches!(
            open,
            BlockKind::FontSize {
                size_type: FontSizeType::Sho,
                ..
            }
        ),
        // 複数行ブロックになれない種類。
        BlockType::Tcy
        | BlockType::Warichu
        | BlockType::Warigaki
        | BlockType::Style
        | BlockType::AnnotationRange
        | BlockType::LeftAnnotationRange => false,
    }
}

/// 終端の並びを、制約 3 の最内層照合で「閉じるもの」と「閉じないもの」に分ける。
///
/// 採用は前から順に決まる（採用するたび最内層が 1 つ外へ動く）ので、開いている種類を
/// 内側から辿って先に決めてしまう。開きを使い切った後の終端は、種類によらず閉じない。
fn split_closes(
    open_kinds: &[BlockKind],
    closes: &[CloseFact],
) -> (Vec<CloseFact>, Vec<CloseFact>) {
    let mut depth = open_kinds.len();
    let mut adopted = Vec::new();
    let mut rejected = Vec::new();
    for close in closes {
        match depth.checked_sub(1).map(|i| &open_kinds[i]) {
            Some(innermost) if end_matches(&close.written, innermost) => {
                depth -= 1;
                adopted.push(*close);
            }
            _ => rejected.push(*close),
        }
    }
    (adopted, rejected)
}

/// 余った終わりのマーカーは `to_inlines` が落とす。
/// 「終わり」を含む行（単独行でないもの）を畳む。
///
/// 参照は行を逐次出力するので、1行に複数の「終わり」があればその順に閉じる
/// （例: `［＃ここで小さな文字終わり］［＃ここで字下げ終わり］`）。各閉じの前の本文は、
/// その時点で開いているブロックの内側に出る。行末の改行を出すのは最後の閉じだけで、
/// 後続本文があるならその行が出す。
///
/// 最内層と種類が一致しない終端（参照実装はエラーで停止する）は閉じずに捨て、診断に出す。
fn apply_closes(
    stack: &mut PlanStack,
    nodes: &[Node],
    closes: &[CloseFact],
    line_no: usize,
    diags: &mut Vec<LowerDiagnostic>,
) {
    let (closes, rejected) = split_closes(&stack.open_kinds(), closes);
    for close in &rejected {
        diags.push(LowerDiagnostic {
            line: line_no,
            kind: LowerDiagnosticKind::UnmatchedEnd {
                written: close.written,
                innermost: stack.innermost().cloned(),
            },
        });
    }
    // 閉じられる終端が1つも無ければ閉じタグは出ないので、行をまとめて内容行にする。
    if closes.is_empty() {
        push_content_line(stack, nodes, line_no);
        return;
    }
    let last_close = closes.last().expect("closes は非空").idx;
    let has_tail = last_close + 1 < nodes.len();
    let mut seg_start = 0usize;

    for (n, CloseFact { idx, explicit, .. }) in closes.iter().enumerate() {
        // 閉じタグより前の本文。行末の改行は閉じタグ以降が出す。
        let segment = (seg_start < *idx).then_some(&nodes[seg_start..*idx]);
        // 参照は閉じタグを buffer に積む（＝本文の続き）ので、本文は閉じるブロックの
        // 内側に出る。ただしぶら下げだけは閉じで indent_stack から降りてしまい
        // per-line の包みが効かなくなるので、その行の本文はブロックの外に出す。
        let closing_burasage = matches!(stack.innermost(), Some(BlockKind::Burasage(_)));
        let is_last = n + 1 == closes.len();
        // 行末の改行を出すのは最後の閉じだけ。後続本文があるなら `</div>` のみ
        // （制約 7 の優先順 2）。
        let closure = Closure::End {
            explicit: *explicit,
            followed_by_content: !is_last || has_tail,
        };

        if closing_burasage {
            stack.close_block(closure);
            if let Some(seg) = segment {
                push_close_segment(stack, seg, Break::NoNewline, line_no);
            }
        } else {
            if let Some(seg) = segment {
                push_close_segment(stack, seg, Break::NoNewline, line_no);
            }
            stack.close_block(closure);
        }
        seg_start = *idx + 1;
    }

    // 最後の閉じの後ろに残った本文を同じ行に出す。
    if has_tail {
        // `Break` の判定に入るのは**閉鎖に採用された終端だけ**。採用されなかった終端は
        // 明示終端でも数えない（実測）。
        let explicit = closes.iter().any(|c| c.explicit);
        let tail = &nodes[last_close + 1..];
        let brk = content_break(&to_inlines(tail), explicit);
        push_close_segment(stack, tail, brk, line_no);
    }
}

/// 内容行として1行を積む。`［＃ここで…終わり］`（explicit_close=true）を含む行は
/// @terprip=false で行末 `<br />` を抑制する（同行開閉の横組み等・複数行ブロックの閉じ行）。
fn push_content_line(stack: &mut PlanStack, nodes: &[Node], line_no: usize) {
    push_content_line_with(stack, Carried::default(), nodes, line_no)
}

/// [`push_content_line`] の、前の行から持ち越した内容を先頭に足す版。
///
/// 持ち越しの由来は [`solve`] の行マージ（参照実装で
/// 出力を飛ばした行のバッファ）。行末 `<br />` の判定は**繋げたあとの行全体**で行う
/// （参照も 1 つのバッファとして `general_output` に渡すため）。
fn push_content_line_with(stack: &mut PlanStack, carried: Carried, nodes: &[Node], line_no: usize) {
    let has_explicit_close = nodes.iter().any(|n| {
        matches!(
            &n.kind,
            NodeKind::BlockEnd {
                explicit_close: true,
                ..
            }
        )
    });
    let mut fragments = carried.fragments;
    fragments.extend(to_inlines(nodes));
    let brk = content_break(&fragments, has_explicit_close);
    stack.push(PlanBlock::Content(PlanLine {
        fragments,
        brk,
        line: line_no,
        source_lines: carried.source_lines,
    }));
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
            stack.close_block(Closure::Implicit(self.close));
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

/// ブロックが何で閉じられたか（制約 7 の優先順を決める入力）。
enum Closure {
    /// 制約 4 の仮想終端。`CloseKind` は表で確定済み。
    Implicit(CloseKind),
    /// 本文中の終端。
    End {
        /// `ここで…終わり` なら true、裸の `…終わり` なら false。
        explicit: bool,
        /// 同じ行でこの閉じの後に、別の閉じか本文が続くか。
        followed_by_content: bool,
    },
    /// EOF の仮想終端（制約 8）。
    Eof,
}

/// 閉じタグの出力形（制約 7）。**優先順のとおりに上から**判定する。
///
/// `outer_after_pop` は閉じたあとに残る最も内側のブロック。ぶら下げの直下かどうかの
/// 判定に要るのでこれを取る（スタックそのものには依存しない）。
///
/// `ここで…終わり`（explicit）は `</div>\r\n`。bare `…終わり` は @terprip 維持で
/// `</div><br />\r\n`（memory bare-block-end）。
fn close_kind(
    closure: &Closure,
    kind: &BlockKind,
    outer_after_pop: Option<&BlockKind>,
) -> CloseKind {
    match closure {
        // 1. 暗黙閉じは制約 4 の表で決まる（一律 NoBreak にしてはいけない。実測）。
        Closure::Implicit(close) => *close,
        // 2. 同一行で最後ではない閉じ、または後続内容を持つ閉じ。
        Closure::End {
            followed_by_content: true,
            ..
        } => CloseKind::NoBreak,
        // 3. EOF で閉じるなら Newline（末尾クローズは行を持たない）。
        Closure::Eof => CloseKind::Newline,
        Closure::End { explicit, .. } => {
            // 4. ぶら下げの直下で閉じるとき、閉じタグが per-line の burasage div に
            //    包まれる種類なら BurasageWrapped。参照が閉じタグを String 扱いして
            //    バッファへ積むため。包む幅は外側のぶら下げが持つので、ここで畳んで
            //    木に載せる（描画器は状態を持たない）。
            if is_burasage_wrapped_close(kind) {
                if let Some(BlockKind::Burasage(geometry)) = outer_after_pop {
                    return CloseKind::BurasageWrapped(*geometry);
                }
            }
            // 5. 残りは明示終端なら Newline、裸の終端なら BareBreak。
            if *explicit {
                CloseKind::Newline
            } else {
                CloseKind::BareBreak
            }
        }
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

    /// 今いちばん内側で開いているブロックの種類。
    fn innermost(&self) -> Option<&BlockKind> {
        self.open.last().map(|b| &b.kind)
    }

    /// 開いているブロックの種類を外側から順に。制約 3 の最内層照合を、実際に閉じる前に
    /// 先読みするために使う（[`split_closes`]）。
    fn open_kinds(&self) -> Vec<BlockKind> {
        self.open.iter().map(|b| b.kind.clone()).collect()
    }

    /// いちばん内側の開いているブロックへ、無ければトップレベルへ積む。
    fn push(&mut self, block: PlanBlock) {
        match self.open.last_mut() {
            Some(b) => b.children.push(block),
            None => self.roots.push(block),
        }
    }

    /// 内容の1行を積む（結合されてきた断片は無い）。
    fn push_line(&mut self, inline: Vec<Inline>, brk: Break, line: usize) {
        self.push(PlanBlock::Content(PlanLine {
            fragments: inline,
            brk,
            line,
            source_lines: Vec::new(),
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

    /// いちばん内側のブロックを閉じて木に載せる。`CloseKind` は制約 7 の導出に任せ、
    /// **ポップ後の**最内側を渡す（ぶら下げ直下かの判定に外側が要る）。
    /// 開いていなければ何もしない。
    fn close_block(&mut self, closure: Closure) {
        let Some(block) = self.open.pop() else {
            return;
        };
        let close = close_kind(&closure, &block.kind, self.innermost());
        self.push(block.into_nested(close));
    }

    /// 閉じられないまま残ったブロックを内側から順に取り出す（EOF 処理用）。
    fn pop_open(&mut self) -> Option<OpenPlanBlock> {
        self.open.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_document_raw;

    fn plan_of(source: &str) -> LowerPlan {
        let lines: Vec<&str> = source.split("\r\n").collect();
        solve(&parse_document_raw(&lines))
    }

    /// Plan の内容行を出現順に集める。
    fn content_breaks(plan: &LowerPlan) -> Vec<Break> {
        fn walk(blocks: &[PlanBlock], out: &mut Vec<Break>) {
            for block in blocks {
                match block {
                    PlanBlock::Content(line) => out.push(line.brk),
                    PlanBlock::Nested { children, .. } => walk(children, out),
                    PlanBlock::LineWrap { .. } => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&plan.roots, &mut out);
        out
    }

    /// 制約 3 の最内層照合: 終端は最内層と種類が一致するときだけ閉じる。
    ///
    /// 期待値は参照実装の `check_close_match` から取った（実測 2026-08-03）。参照は
    /// 書かれた種類と `@indent_stack.last` を比べ、違えば「〈種類〉を閉じようとしましたが、
    /// 〈種類〉中ではありません」で停止する。細部 2 つも参照から:
    /// ぶら下げは `hanging_indent?` が `:jisage` を返すので字下げの終端で閉じ、
    /// 大きな文字と小さな文字は別のキー（`dai`/`sho`）なので段階違いは一致しない。
    #[test]
    fn end_closes_only_when_the_innermost_kind_matches() {
        let cases = [
            // 一致するので閉じる（従来どおり）。
            (
                "［＃ここから太字］\r\n本文\r\n［＃ここで太字終わり］\r\nあと",
                "本文<br />\r\n</div>",
            ),
            // bare 終端も種類が一致すれば閉じる（参照も停止しない形）。
            (
                "［＃ここから２字下げ］\r\n本文\r\n［＃字下げ終わり］\r\nあと",
                "本文<br />\r\n</div><br />",
            ),
            // ぶら下げは字下げの終端で閉じる（参照 hanging_indent?）。
            (
                "［＃ここから改行天付き、折り返して３字下げ］\r\n本文\r\n［＃ここで字下げ終わり］\r\nあと",
                "本文</div>\r\nあと",
            ),
            // 種類が食い違うので閉じない。太字は EOF まで残る。
            (
                "［＃ここから太字］\r\n本文\r\n［＃ここで字下げ終わり］\r\nあと",
                "本文<br />\r\nあと<br />\r\n</div>",
            ),
            // 大小の段階違いも一致しない（別キー）。
            (
                "［＃ここから１段階大きな文字］\r\n本文\r\n［＃ここで小さな文字終わり］\r\nあと",
                "本文<br />\r\nあと<br />\r\n</div>",
            ),
        ];
        for (body, expected) in cases {
            let html = crate::html::convert(
                &format!("題\r\n著\r\n\r\n{body}\r\n底本：テスト\r\n"),
                &crate::html::RenderOptions::default(),
            );
            assert!(html.contains(expected), "{body:?} → {html}");
        }
    }

    /// 制約 1 の規則 5: 行途中の開始は、**開始マーカーの前に何があっても**開く。
    ///
    /// 参照実装で 5 形とも実測した（2026-08-03。いずれも参照は正常終了する＝正解のある
    /// 差である）。かつては「前がすべてテキスト」のときだけ開いており、ルビ・外字・傍点・
    /// 行内地付きが前にあると開始が黙って消えて書式が丸ごと落ちていた。
    ///
    /// 最後の 1 形だけは範囲が参照と違い、**追随しない**。参照は地付きを 2 行に伸ばして
    /// 斜体を 1 行で切るが、記法の定義（`［＃地付き］` は行スコープ、`ここから…終わり` は
    /// 行をまたぐ）に沿うのはこちらである。参照自身も一貫しておらず、この形で
    /// `［＃ここで地付き終わり］` は「地付き中ではありません」で拒否する（実測）。
    /// 詳細と判断は docs/spec-lowerer-constraints.md「行中の `ここから…` に追随しない範囲」。
    /// この形には診断 `midline-block-open`（Warning）を出す。
    #[test]
    fn mid_line_open_ignores_what_precedes_the_marker() {
        let cases = [
            (
                "本文［＃ここから斜体］つづき",
                "本文<div class=\"shatai\">つづき",
            ),
            (
                "東京《とうきょう》［＃ここから斜体］つづき",
                "</ruby><div class=\"shatai\">つづき",
            ),
            (
                "※［＃「けものへん＋苗」、第3水準1-87-63］［＃ここから斜体］つづき",
                "class=\"gaiji\" /><div class=\"shatai\">つづき",
            ),
            (
                "本文［＃「本文」に傍点］［＃ここから斜体］つづき",
                "<em class=\"sesame_dot\">本文</em><div class=\"shatai\">つづき",
            ),
            (
                "前［＃地付き］本文［＃ここから斜体］つづき",
                "本文</div><div class=\"shatai\">つづき",
            ),
        ];
        for (source, expected) in cases {
            let html = crate::html::convert(
                &format!("題\r\n著\r\n\r\n{source}\r\n底本：テスト\r\n"),
                &crate::html::RenderOptions::default(),
            );
            assert!(html.contains(expected), "{source} → {html}");
        }
    }

    /// 制約 3: 閉鎖に採用されなかった終端（`IgnoredEnd`）は `Break` の判定に入らない。
    /// ただし行の終端が一つも採用されなかったときは、行全体が内容行になり、行内の
    /// すべての明示終端が判定に入る。設計文書に載る実測 2 例をそのまま固定する。
    #[test]
    fn ignored_ends_do_not_count_for_break() {
        // 開いている字下げが 1 つ。裸の終端だけが採用され、残った明示終端は効かない
        // ので、`Ｃ` の後に `<br />` が出る（Break::Br）。
        let plan =
            plan_of("［＃ここから２字下げ］\r\nＡ［＃字下げ終わり］Ｂ［＃ここで字下げ終わり］Ｃ");
        assert_eq!(
            content_breaks(&plan).last(),
            Some(&Break::Br),
            "採用されなかった明示終端が Break を抑制してしまっている"
        );

        // 開いているブロックが無い。行全体が内容行になり、明示終端が効いて
        // `<br />` が出ない（Break::None）。
        let plan = plan_of("本文［＃ここで字下げ終わり］あと");
        assert_eq!(
            content_breaks(&plan).last(),
            Some(&Break::None),
            "閉じられる開きが無い行では明示終端が Break に効くはず"
        );
    }
}
