//! Aozora AST（block ⊃ line ⊃ inline の木）
//!
//! 青空文書の意味構造を表す正規モデル。バックエンド（HTML／プレーンテキスト）が
//! 消費するクリーンな木で、HTML でもテキストでも描ける性質（backend-neutral。
//! 旧称「中立AST」）を持つ。RawAST（[`crate::node::Node`] の平坦マーカー列）を
//! Lowerer（[`crate::lower::lower_to_blocks`]）が畳んで作る（前方参照解決済み・
//! ブロックは部分木・行末の改行は互換メタデータ [`Break`]/[`CloseKind`]）。
//!
//! 本番の HTML／プレーンテキスト変換はこの木のみを経由する。仕様は
//! docs/spec-ast.md、移行の経緯は docs/plan-neutral-ast.md。
//!
//! RawAST の [`crate::node::Node`] とは別型にすることで、バックエンドが
//! ソース文字列や `BlockStart`/`BlockEnd` マーカーを見られないようにする
//! （architecture.md §4.2 型の壁）。
//!
//! ここは**型と不変条件だけ**を置く。RawAST からの畳み込みは Lowerer
//! （[`crate::lower`]。ブロック層は `lower::lower_to_blocks`、インライン層は
//! `lower::inline::to_inlines`）、HTML 出力形に由来する判断は
//! `lower::break_policy` にある。

use crate::node::{FontSizeType, MidashiLevel, MidashiStyle, RubyDirection, StyleType};
use crate::token::Span;

/// 文書全体の Aozora AST ＝トップレベル [`Block`] の列。
///
/// 「文書 1 本＝ブロックの木の列」を表す型名（backend-neutral な正規モデル。
/// 旧称「中立AST」）。**文書 1 本まるごと**を指す値にだけ使う
/// （[`crate::lower::lower_to_blocks`] の返り値と、それを組み立てる器）。
/// ブロックの断片＝入れ子の子ブロック列や作業用スタックは素の `Vec<Block>` のまま。
pub type AozoraAst = Vec<Block>;

/// ブロック（部分木の節、または内容の1行）。
///
/// 青空文庫記法の論理構造は「ブロック ⊃ 行 ⊃ インライン」の3層。
/// 「ブロックだけの行」（`［＃ここから…］` など）は器（[`Block::Nested`]）に
/// 吸収され、`<br/>` 特別扱いの多くが構造から自然に従う。
#[derive(Debug, Clone)]
pub enum Block {
    /// 内容の1行。インライン列と、行末の改行制御（互換メタデータ）を持つ。
    Line {
        /// 行の内容（インライン列）
        inline: Vec<Inline>,
        /// 行末の改行制御
        brk: Break,
        /// この行の由来（本文 0 起点の行番号）＝位置情報。
        line: usize,
    },
    /// 入れ子ブロック（字下げ・地付き・字詰め・ぶら下げ・見出し・罫囲み等）。
    Nested {
        /// ブロックの種類とパラメータ
        kind: BlockKind,
        /// 子ブロック列
        children: Vec<Block>,
        /// 閉じタグの出力形（互換メタデータ）。参照実装は1ソース行につき1つの `\r\n`
        /// を出すので、閉じの改行・`<br />` はどの契機で閉じたかで決まる（[`CloseKind`]）。
        close: CloseKind,
        /// 開始タグの出力形（互換メタデータ）。[`CloseKind`] と対をなす。
        open: OpenKind,
        /// このブロックを開いた本文行の番号（0 起点）＝位置情報。
        line: usize,
    },
    /// 行単位のブロック包み（同じ行に本文がある字下げ／地付き等）。
    /// 参照実装 apply_jisage / 行スコープ地付き（is_block=false）は行全体を1行の
    /// div で包む：`<div class="…">{inline}</div>\r\n`。複数行 Nested と違い、
    /// 開きタグ直後の `\r\n` も内側 `<br />` も出ない。
    LineWrap {
        /// ブロックの種類（Jisage / Chitsuki など）
        kind: BlockKind,
        /// 行の内容（インライン列）
        inline: Vec<Inline>,
        /// この行の由来（本文 0 起点の行番号）＝位置情報。
        line: usize,
    },
}

/// 入れ子ブロックの開始タグの出力形（互換メタデータ）。
///
/// 参照実装は1ソース行につき `\r\n` を1つだけ出す。ブロックが行頭で開くときは
/// 開始タグがその行の唯一の出力なので改行が付くが、行の途中で開くときは同じ行に
/// 内容が続くので改行は付かない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenKind {
    /// 開始タグの後に `\r\n` を出す（行頭で開く複数行ブロック）。
    Newline,
    /// 開始タグの後に改行を出さない（行の途中で開き、同じ行に内容が続く）。
    NoBreak,
}

/// 入れ子ブロックの閉じタグの出力形（互換メタデータ）。参照実装 general_output の
/// `@terprip` 判定を Lower 時に確定させたもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseKind {
    /// `</div>`（改行なし）。次の開始タグや本文が同じ出力行に続く暗黙閉じ
    /// （兄弟 jisage・行スコープ chitsuki の implicit_close、先頭 BlockEnd＋後続本文）。
    NoBreak,
    /// `</div>\r\n`。`ここで…終わり`（explicit）・burasage 開始の暗黙閉じ・EOF 閉じ。
    Newline,
    /// `</div><br />\r\n`。bare `…終わり`（`ここで` 無し）で複数行ブロックを閉じた行。
    /// 参照は @terprip を維持するので行末 `<br />` が付く（memory bare-block-end）。
    BareBreak,
    /// `<div class="burasage" …></div></div>\r\n`。ぶら下げの直下で装飾系ブロック
    /// （横組み・罫囲み・キャプション・文字サイズ・太字・斜体・字詰め）が閉じる行。
    ///
    /// 参照実装は閉じタグを String としてバッファに積むため blank_type が false になり、
    /// その行を per-line の burasage div で包む（＝閉じタグが div の中に入る）。
    /// 包む幅は外側のぶら下げが持つものなので、描画器が状態を持たずに出せるよう
    /// Lower 時にここへ畳んでおく。
    BurasageWrapped(BurasageGeometry),
}

/// ぶら下げ（折り返し字下げ）の幅。
///
/// [`BlockKind::Burasage`] と、その直下で装飾系ブロックが閉じる行の
/// [`CloseKind::BurasageWrapped`] が**同じ値**を使う（後者は外側ぶら下げの幅で
/// 閉じタグを包む）。取り違えないよう1つの型にまとめている。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BurasageGeometry {
    /// margin-left em（折り返し字下げ幅。参照 wrap_width）。空（コンマなし）のとき
    /// None → margin-left を空文字（Quirk empty_indent_css）にする。
    pub wrap_width: Option<u32>,
    /// 字下げ幅 em（空のとき None＝0 扱い）。
    pub width: Option<u32>,
}

impl BurasageGeometry {
    /// text-indent em（字下げ幅 − 折り返し幅）。空幅はどちらも 0 とみなす。
    pub fn text_indent(&self) -> i32 {
        self.width.unwrap_or(0) as i32 - self.wrap_width.unwrap_or(0) as i32
    }
}

/// 入れ子ブロックの種類。`ここから…` で開く複数行ブロックに対応する。
///
/// 参照実装 INDENT_TYPE ＋ ブロック化しうるスタイルに対応。幅・レベル等の
/// パラメータは種類ごとに持つ（RawAST の `BlockParams` を種類別に畳んだもの）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    /// 字下げ（N字下げ）。幅 em。幅が空（`［＃ここから字下げ］` の数字なし）のとき
    /// None。参照は空幅で `class="jisage_" style="margin-left: em"`（不正CSS）を出す
    /// （Quirk empty_indent_css）。
    Jisage { width: Option<u32> },
    /// 地付き／字上げ（chitsuki）。右寄せ、右マージン em。
    Chitsuki { width: u32 },
    /// 字詰め。幅 em。
    Jizume { width: u32 },
    /// ぶら下げ（折り返し字下げ）。参照実装の per-line モデルでは外側 div を作らず、
    /// 各内容行を個別の `<div class="burasage" style="margin-left: {wrap_width}em;
    /// text-indent: {text_indent}em;">` で包む（空行は素の `<br />`）。
    Burasage(BurasageGeometry),
    /// 見出し（後続の行を包む）。
    Midashi {
        level: MidashiLevel,
        style: MidashiStyle,
    },
    /// 罫囲み（ブロック形）。
    Keigakomi,
    /// 横組み（ブロック形）。
    Yokogumi,
    /// キャプション（ブロック形）。
    Caption,
    /// 大きな文字／小さな文字（ブロック形）。
    FontSize { size_type: FontSizeType, level: u32 },
    /// 太字（ブロック形）。
    Futoji,
    /// 斜体（ブロック形）。
    Shatai,
}

/// 行末の改行制御（互換ストリーミングモデルのメタデータ）。
///
/// 参照実装 `general_output` の `@terprip` / tail 判断を Lower 時に畳んだ結果。
/// バックエンドはこれを消費するだけで、行末の `<br/>` 有無を状態なしに決められる
/// （architecture.md §4.3）。今後 Phase B3 で bare ブロック終了行の `</div><br/>`
/// など細則を足しうる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Break {
    /// 行末に `<br />` を出す（`@terprip=true` の通常行）。
    Br,
    /// 行末に `<br />` を出さない（`@terprip=false`。`ここで…終わり`・見出し等）。
    None,
    /// `<br />` も行末の `\r\n` も出さない。行の途中でブロックが閉じるとき、
    /// 閉じタグより前の本文に使う（改行は閉じタグ側が出す）。
    NoNewline,
}

/// インライン要素（行の内容の葉・入れ子）。
///
/// RawAST の [`crate::node::Node`] のうちインラインに相当する変種を、前方参照
/// 解決済み・子を `Vec<Inline>` にした形で写す。`BlockStart`/`BlockEnd`/
/// `LineJisage`/`UnresolvedReference` 等のマーカー系はAozora ASTには現れない。
#[derive(Debug, Clone, PartialEq)]
pub enum InlineKind {
    /// プレーンテキスト
    Text(String),
    /// ルビ
    Ruby {
        base: Vec<Inline>,
        ruby: Vec<Inline>,
        direction: RubyDirection,
        /// 親文字内の外字注記を rb 内に残すか（`crate::node::NodeKind::Ruby` 参照）。
        keep_gaiji_notes_in_base: bool,
    },
    /// 装飾（傍点・傍線・太字・斜体など）
    Style {
        children: Vec<Inline>,
        style_type: StyleType,
    },
    /// 見出し（同行・窓見出し＝インライン見出し。`ここから…見出し` は BlockKind 側）
    Midashi {
        children: Vec<Inline>,
        level: MidashiLevel,
        style: MidashiStyle,
    },
    /// 左注記範囲の終了マーカー（外字を含みうる。`crate::node::NodeKind::AnnotationEnd`）
    AnnotationEnd {
        prefix: String,
        content: Vec<Inline>,
        suffix: String,
    },
    /// 外字
    Gaiji {
        description: String,
        unicode: Option<String>,
        jis_code: Option<String>,
        had_igeta: bool,
    },
    /// アクセント分解文字
    Accent {
        code: String,
        name: String,
        unicode: Option<String>,
    },
    /// 画像
    Img {
        filename: String,
        alt: String,
        is_photo: bool,
        width: Option<u32>,
        height: Option<u32>,
    },
    /// 縦中横（インライン）
    Tcy { children: Vec<Inline> },
    /// 罫囲み（インライン）
    Keigakomi { children: Vec<Inline> },
    /// 横組み（インライン）
    Yokogumi { children: Vec<Inline> },
    /// キャプション（インライン）
    Caption { children: Vec<Inline> },
    /// 割り注（warichu）。参照 apply_warichu は状態を持たず開閉を素の文字列で
    /// 出すため、Aozora ASTでは開き `（`／閉じ `）` を持つマーカーとして表す。
    Warichu {
        /// 開きか閉じか（true=開き `<span class="warichu">（`, false=閉じ `）</span>`）
        open: bool,
        /// 直前が `（` で終わる／直後が `）` で始まる場合の括弧重複回避
        suppress_paren: bool,
    },
    /// 割書（warigaki, `<span class="warigaki">`）
    Warigaki { children: Vec<Inline> },
    /// フォントサイズ（インライン）
    FontSize {
        children: Vec<Inline>,
        size_type: FontSizeType,
        level: u32,
    },
    /// 返り点（中身は素の文字。参照実装も再パースしない）
    Kaeriten(String),
    /// 訓点送り仮名（`<sup class="okurigana">`）。中身は [`InlineKind::Note`] と
    /// 同じく解決済み。描画時に外側の `（ ）` を落とす（参照 Tag::Okurigana）。
    Okurigana {
        /// 解決済みの中身
        content: Vec<Inline>,
        /// 元の注記文字列（診断・エディタ支援用）
        raw: String,
    },
    /// 注記（編集者注 `<span class="notes">［＃…］</span>`）。
    ///
    /// 参照実装は注記の中身を別の TagParser に渡して描画する（中のルビ・外字が
    /// 解決される）。その再パースは Lowerer が一度だけ行い、ここには**解決済みの
    /// インライン列**が入る（バックエンドがトークナイザ・パーサに依存しないため）。
    ///
    /// `content` の各 span は**注記文字列内の相対位置**（`raw` の先頭が 0）。
    /// 前方参照の解決に失敗した注記など `raw` がソース行の部分文字列でない
    /// （合成された）ものがあるため、行内の絶対位置には写せない。行内の位置が
    /// 要るときは注記自身（[`Inline::span`]）を使う。
    Note {
        /// 解決済みの中身
        content: Vec<Inline>,
        /// 元の注記文字列（診断・エディタ支援用）
        raw: String,
    },
    /// 濁点片仮名（面区点 1-7-82〜85）
    DakutenKatakana { num: String },
    /// 行の途中で開く地付き／字上げ（`TEXT［＃地付き］attribution`）。
    /// 参照 close_inline_blocks は行末で閉じるので、マーカー以降を行末まで
    /// `<div class="chitsuki_N" style="text-align:right; margin-right: Nem">…</div>`
    /// で包む（後続に <br /> は付かない＝ブロックのみ行扱い）。
    ChitsukiInline { width: u32, children: Vec<Inline> },
    /// 同一行で開閉するブロック形コマンド（`TEXT［＃ここから横組み］…［＃ここで横組み
    /// 終わり］TEXT`）。参照は is_block=true のブロック開始タグ（div/h4）を行内に埋め
    /// 込み、行末の close_inline_blocks／同行終わりで閉じる。開き直後の `\r\n` や
    /// 内側 `<br />` は出ない（インライン埋め込み）。
    BlockInline {
        kind: BlockKind,
        children: Vec<Inline>,
    },
}

/// Aozora ASTのインライン要素。各要素が位置範囲を自前で持つ。
///
/// span は原則としてソース**行内**の絶対 char 位置だが、例外がある。
/// [`InlineKind::Note`]/[`InlineKind::Okurigana`] の `content` は注記文字列を
/// 再パースしたものなので、その中の span は**注記文字列内の相対位置**になる
/// （合成された注記は行の部分文字列とは限らず、行内絶対位置に写せない）。
/// 位置として使う側は注記の内側に降りたかどうかを意識すること。
/// 詳しくは docs/spec-ast.md「span が『実在』でない箇所」。
#[derive(Debug, Clone)]
pub struct Inline {
    /// インライン種別と内容。
    pub kind: InlineKind,
    /// 位置範囲（原則はソース行内の絶対 char 位置。注記の中身だけ相対＝型の doc 参照）。
    pub span: Span,
    /// 範囲形（`［＃中見出し］…［＃中見出し終わり］`）由来か。後方参照形
    /// （`［＃「…」は中見出し］`）なら false。
    ///
    /// 互換メタデータ。参照実装は範囲形の中身をバッファに**素の String** として
    /// 残すが、後方参照形は String をタグへ取り込んで消す。この差はぶら下げの
    /// per-line 包み（`TextBuffer#blank_type`）の判定に効く。
    pub range_form: bool,
}

impl Inline {
    /// 種別とソース位置からインラインを作成する（後方参照形＝`range_form: false`）。
    pub fn new(kind: InlineKind, span: Span) -> Self {
        Self {
            kind,
            span,
            range_form: false,
        }
    }

    /// 範囲形（`［＃…］…［＃…終わり］`）由来として作成する。
    pub fn from_range(kind: InlineKind, span: Span) -> Self {
        Self {
            kind,
            span,
            range_form: true,
        }
    }

    /// テキストインラインを作成する。
    pub fn text(s: impl Into<String>, span: Span) -> Self {
        Self::new(InlineKind::Text(s.into()), span)
    }
}

/// span は位置メタデータなので構造比較に含めない。`range_form` は出力形を変える
/// 互換メタデータ（[`CloseKind`]/[`OpenKind`] と同類）なので**含める**。
impl PartialEq for Inline {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.range_form == other.range_form
    }
}

/// 由来行 `line` は位置メタデータであり、構造比較には含めない
/// （[`Inline`] が span を含めないのと同じ方針）。
impl PartialEq for Block {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Block::Line { inline, brk, .. },
                Block::Line {
                    inline: other_inline,
                    brk: other_brk,
                    ..
                },
            ) => inline == other_inline && brk == other_brk,
            (
                Block::Nested {
                    kind,
                    children,
                    close,
                    open,
                    ..
                },
                Block::Nested {
                    kind: other_kind,
                    children: other_children,
                    close: other_close,
                    open: other_open,
                    ..
                },
            ) => {
                kind == other_kind
                    && children == other_children
                    && close == other_close
                    && open == other_open
            }
            (
                Block::LineWrap { kind, inline, .. },
                Block::LineWrap {
                    kind: other_kind,
                    inline: other_inline,
                    ..
                },
            ) => kind == other_kind && inline == other_inline,
            // 種類が違えば不等。`_ => false` と書くと変種を足したとき
            // **同じ値どうしが不等**になり（反射律違反）、しかも気付けない。
            // 左側だけを列挙すればタプル空間を網羅でき、変種追加が
            // コンパイルエラーになる。
            (Block::Line { .. }, _) | (Block::Nested { .. }, _) | (Block::LineWrap { .. }, _) => {
                false
            }
        }
    }
}
