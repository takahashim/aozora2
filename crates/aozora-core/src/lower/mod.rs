//! Lowerer: RawAST（平坦マーカー）→ Aozora AST（block ⊃ line ⊃ inline の木）
//!
//! architecture.md §4.1/§4.3・docs/plan-neutral-ast.md（Phase B2〜）。
//! 参照実装 `@indent_stack`/`implicit_close`/`@terprip` の逐次モデルを Lower 時に
//! 一度だけ計算し、ブロックを部分木に畳み、行末の改行を [`Break`] メタデータへ
//! 載せる。バックエンドはこの木を状態なしに歩くだけになる。
//!
//! 畳み込みの状態は [`BlockStack`]（開いているブロックのスタック＋トップレベル列）に
//! 閉じてあり、各行は [`classify_line`] が返す [`LineKind`] ごとにそこへ積む。
//! [`block_kind_of`] が写せないブロック種はトップレベルに落ちる。

pub mod break_policy;
pub mod inline;

use break_policy::content_break;
use inline::to_inlines;

use crate::ast::{
    AozoraAst, Block, BlockKind, Break, BurasageGeometry, CloseKind, Inline, OpenKind,
};
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
    let mut stack = BlockStack::new();
    let mut diags: Vec<LowerDiagnostic> = Vec::new();

    for raw_line in &raw.lines {
        let line_no = raw_line.line_no;
        // 前方参照とルビ親文字を解決してから畳む（旧経路と同順）。span は畳み込みに使わない。
        let mut nodes = raw_line.nodes.clone();
        resolve_references(&mut nodes);
        resolve_inline_ruby(&mut nodes);

        match classify_line(&nodes) {
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
                let rest = strip_line_scope_marker(nodes);
                stack.push(Block::LineWrap {
                    kinds,
                    inline: to_inlines(&rest),
                    line: line_no,
                });
            }
            LineKind::Content => push_content_line(&mut stack, &nodes, line_no),
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
    let has_explicit_close = nodes.iter().any(|n| {
        matches!(
            &n.kind,
            NodeKind::BlockEnd {
                explicit_close: true,
                ..
            }
        )
    });
    let inline = to_inlines(nodes);
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
        kind: NodeKind::BlockEnd { explicit_close, .. },
        ..
    }] = nodes
    {
        return LineKind::BlockClose(*explicit_close);
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
        BlockType::Jizume => Some(BlockKind::Jizume {
            width: params.width,
        }),
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
        BlockType::Burasage => Some(BlockKind::Burasage(BurasageGeometry {
            wrap_width: params.wrap_width,
            width: params.width,
        })),
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

    /// 行末クローズの前半は Text だけとは限らない（行途中の地付き・ルビ等）。
    /// 拾うのは「同じ行に対応する開始が無い BlockEnd」で、同じ行で開閉する範囲形は
    /// to_inlines がインラインに畳むので対象外。
    ///
    /// 例: 001848/59607 の
    /// `ウェヌス…蒔《ま》かんとする時、［＃地から２字上げ］（ルクレティウス）［＃ここで字下げ終わり］`
    #[test]
    fn block_end_closes_even_when_head_has_inline_markers() {
        let lines = vec![
            "［＃ここから２字下げ］",
            "本文《ほんぶん》。［＃地から２字上げ］（出典）［＃ここで字下げ終わり］",
        ];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        match &blocks[0] {
            Block::Nested { kind, close, .. } => {
                assert_eq!(*kind, BlockKind::Jisage { width: Some(2) });
                assert_eq!(*close, CloseKind::Newline);
            }
            other => panic!("Nested を期待: {other:?}"),
        }
    }

    /// 複数行ブロックになれない種類の「終わり」でブロックを閉じない。割り注は
    /// 開始側が注記として描画され BlockStart ノードを作らないので、素朴に
    /// 「対応する開始が無い」と見ると外側の字下げを誤って閉じてしまう
    /// （000284/2227 で実際に起きた）。
    #[test]
    fn warichu_end_does_not_close_the_enclosing_block() {
        let lines = vec![
            "［＃ここから３字下げ］",
            "本文。［＃ここから割り注］注［＃ここで割り注終わり］続き",
            "［＃ここで字下げ終わり］",
        ];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        match &blocks[0] {
            Block::Nested { children, .. } => {
                assert_eq!(children.len(), 1, "割り注の行は字下げの中に留まる");
            }
            other => panic!("Nested を期待: {other:?}"),
        }
    }

    /// 1行に「終わり」が複数あれば現れた順に閉じる。行末の改行を出すのは最後の
    /// 閉じだけなので `</div></div>\r\n` になる。
    ///
    /// 例: 001097/49825 の `［＃ここで小さな文字終わり］［＃ここで字下げ終わり］`
    #[test]
    fn multiple_block_ends_on_one_line_close_in_order() {
        let lines = vec![
            "［＃ここから７字下げ］",
            "［＃ここから１段階小さな文字］",
            "本文",
            "［＃ここで小さな文字終わり］［＃ここで字下げ終わり］",
        ];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            // 外側の字下げが最後に閉じるので改行はこちらが出す。
            Block::Nested {
                kind,
                close,
                children,
                ..
            } => {
                assert_eq!(*kind, BlockKind::Jisage { width: Some(7) });
                assert_eq!(*close, CloseKind::Newline);
                match &children[0] {
                    // 内側の小さな文字は改行なしの `</div>`。
                    Block::Nested { close, .. } => assert_eq!(*close, CloseKind::NoBreak),
                    other => panic!("Nested を期待: {other:?}"),
                }
            }
            other => panic!("Nested を期待: {other:?}"),
        }
    }

    /// 行途中の地付き（is_block=false の Chitsuki）は行末で閉じるので、同じ行の
    /// `［＃ここで地付き終わり］` は前の行から続く複数行の地付きを閉じる。
    ///
    /// 参照実装で実測（`Ａ［＃地付き］Ｂ［＃ここで地付き終わり］` は
    /// `Ａ<div class="chitsuki_0">Ｂ</div></div>` になる）。
    #[test]
    fn inline_chitsuki_does_not_absorb_the_multiline_chitsuki_end() {
        let lines = vec![
            "［＃ここから地付き］",
            "本文",
            "Ａ［＃地付き］Ｂ［＃ここで地付き終わり］",
            "後の行",
        ];
        let blocks = lower_to_blocks(&parse_document_raw(&lines));
        match &blocks[0] {
            Block::Nested { kind, close, .. } => {
                assert_eq!(*kind, BlockKind::Chitsuki { width: 0 });
                assert_eq!(*close, CloseKind::Newline, "この行で閉じる");
            }
            other => panic!("Nested を期待: {other:?}"),
        }
        assert!(
            matches!(blocks[1], Block::Line { line: 3, .. }),
            "後の行はブロックの外: {:?}",
            blocks[1]
        );
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

    /// `［＃N字下げ］` は 1 行に複数書ける。参照 apply_jisage は見つけるたび
    /// バッファ先頭へ unshift するので、**後に書いたものほど外側**の div になる。
    /// また `｜` の直後に書くとトークナイザがルビ親文字へ取り込むが、参照は
    /// ルビの状態に関わらず行を包む。どちらも参照実装で実測した。
    #[test]
    fn line_jisage_can_repeat_and_hide_in_ruby_base() {
        let body = |line: &str| {
            let src = format!("作品名\r\n著者\r\n\r\n{line}\r\n\r\n底本：「テスト」\r\n");
            crate::html::convert(&src, &crate::html::RenderOptions::default())
        };
        // 後に書いた ５字下げ が外側。
        let twice = body("［＃２字下げ］あ［＃５字下げ］い");
        assert!(
            twice.contains(
                "<div class=\"jisage_5\" style=\"margin-left: 5em\">\
                 <div class=\"jisage_2\" style=\"margin-left: 2em\">あい</div></div>"
            ),
            "{twice}"
        );
        // ｜ の直後（ルビ親文字の中）でも行を包む。
        let in_ruby = body("｜［＃２字下げ］あいう《るび》");
        assert!(
            in_ruby.contains("<div class=\"jisage_2\" style=\"margin-left: 2em\"><ruby>"),
            "{in_ruby}"
        );
        assert!(in_ruby.contains("</ruby></div>"), "{in_ruby}");
    }

    /// 幅の数字が無い `［＃字下げ］` `［＃ここから字詰め］` も参照は受理し、
    /// 空幅のまま不正な CSS を出す（Quirk empty_indent_css）。注記化すると
    /// div ごと消えてしまう。期待値は参照実装で実測した。
    #[test]
    fn empty_width_indent_commands_are_accepted() {
        let body = |line: &str| {
            let src = format!("作品名\r\n著者\r\n\r\n{line}\r\n\r\n底本：「テスト」\r\n");
            crate::html::convert(&src, &crate::html::RenderOptions::default())
        };
        let jisage = body("［＃字下げ］あいう");
        assert!(
            jisage.contains("<div class=\"jisage_\" style=\"margin-left: em\">あいう</div>"),
            "{jisage}"
        );
        let jizume = body("［＃ここから字詰め］\r\nあいう\r\n［＃ここで字詰め終わり］");
        assert!(
            jizume.contains("<div class=\"jizume_\" style=\"width: em\">"),
            "{jizume}"
        );
    }

    /// `［＃N字下げ］` や行スコープ地付きは、同じ行に `［＃ここで…終わり］` が
    /// あっても行を包む。classify_line は「終わり」を含む行を先に Closes として
    /// 扱うので、行スコープ包みの判定まで来ないのが原因だった。
    /// 参照 apply_jisage は閉じの有無に関わらずバッファへ unshift するだけ（実測）。
    #[test]
    fn line_scope_wrap_survives_a_close_on_the_same_line() {
        let body = |lines: &[&str]| {
            let src = format!(
                "作品名\r\n著者\r\n\r\n{}\r\n\r\n底本：「テスト」\r\n",
                lines.join("\r\n")
            );
            crate::html::convert(&src, &crate::html::RenderOptions::default())
        };
        let jisage = body(&[
            "［＃ここから２字下げ、折り返して４字下げ］",
            "まえ。",
            "［＃２字下げ］あいう［＃ここで字下げ終わり］",
        ]);
        assert!(
            jisage.contains("<div class=\"jisage_2\" style=\"margin-left: 2em\">あいう</div>"),
            "{jisage}"
        );
        let chitsuki = body(&[
            "［＃ここから２字下げ］",
            "まえ。",
            "［＃地付き］あいう［＃ここで字下げ終わり］",
        ]);
        assert!(
            chitsuki.contains(
                "chitsuki_0\" style=\"text-align:right; margin-right: 0em\">あいう</div>"
            ),
            "{chitsuki}"
        );
    }
}
