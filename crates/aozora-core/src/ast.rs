//! 中立AST（block ⊃ line ⊃ inline の木）
//!
//! architecture.md §4.1 の目標である、バックエンドが消費するクリーンな木。
//! RawAST（[`crate::node::Node`] の平坦マーカー列）を Lowerer が畳んで作る
//! （前方参照解決済み・ブロックは部分木・行末の改行は互換メタデータ [`Break`]）。
//!
//! この型はまだパイプラインに接続されていない（Phase B1: 型の定義のみ）。
//! Lowerer（`lower_to_blocks`）と新バックエンドは後続の Phase で接続する。
//! 実行計画: docs/plan-neutral-ast.md。
//!
//! RawAST の [`crate::node::Node`] とは別型にすることで、バックエンドが
//! ソース文字列や `BlockStart`/`BlockEnd` マーカーを見られないようにする
//! （architecture.md §4.2 型の壁）。

use crate::node::{FontSizeType, MidashiLevel, MidashiStyle, RubyDirection, StyleType};

/// ブロック（部分木の節、または内容の1行）。
///
/// 青空文庫記法の論理構造は「ブロック ⊃ 行 ⊃ インライン」の3層。
/// 「ブロックだけの行」（`［＃ここから…］` など）は器（[`Block::Nested`]）に
/// 吸収され、`<br/>` 特別扱いの多くが構造から自然に従う。
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// 内容の1行。インライン列と、行末の改行制御（互換メタデータ）を持つ。
    Line {
        /// 行の内容（インライン列）
        inline: Vec<Inline>,
        /// 行末の改行制御
        brk: Break,
    },
    /// 入れ子ブロック（字下げ・地付き・字詰め・ぶら下げ・見出し・罫囲み等）。
    Nested {
        /// ブロックの種類とパラメータ
        kind: BlockKind,
        /// 子ブロック列
        children: Vec<Block>,
    },
}

/// 入れ子ブロックの種類。`ここから…` で開く複数行ブロックに対応する。
///
/// 参照実装 INDENT_TYPE ＋ ブロック化しうるスタイルに対応。幅・レベル等の
/// パラメータは種類ごとに持つ（RawAST の `BlockParams` を種類別に畳んだもの）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    /// 字下げ（N字下げ）。幅 em。
    Jisage { width: u32 },
    /// 地付き／字上げ（chitsuki）。右寄せ、右マージン em。
    Chitsuki { width: u32 },
    /// 字詰め。幅 em。
    Jizume { width: u32 },
    /// ぶら下げ（折り返し字下げ）。左マージンと字下げ（text-indent）em。
    /// 参照実装の per-line モデルでは各子行を個別の div で包む。
    Burasage {
        /// margin-left em（コンマなしで幅が空のとき None）
        margin: Option<u32>,
        /// text-indent em（負値）
        text_indent: i32,
    },
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
}

/// インライン要素（行の内容の葉・入れ子）。
///
/// RawAST の [`crate::node::Node`] のうちインラインに相当する変種を、前方参照
/// 解決済み・子を `Vec<Inline>` にした形で写す。`BlockStart`/`BlockEnd`/
/// `LineJisage`/`UnresolvedReference` 等のマーカー系は中立ASTには現れない。
#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    /// プレーンテキスト
    Text(String),
    /// ルビ
    Ruby {
        base: Vec<Inline>,
        ruby: Vec<Inline>,
        direction: RubyDirection,
        /// 親文字内の外字注記を rb 内に残すか（`crate::node::Node::Ruby` 参照）。
        keep_gaiji_notes_in_base: bool,
    },
    /// 装飾（傍点・傍線・太字・斜体など）
    Style {
        children: Vec<Inline>,
        style_type: StyleType,
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
    /// 出すため、中立ASTでは開き `（`／閉じ `）` を持つマーカーとして表す。
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
    /// 返り点
    Kaeriten(String),
    /// 訓点送り仮名
    Okurigana(String),
    /// 注記（編集者注 `<span class="notes">［＃…］</span>`）
    Note(String),
    /// 濁点片仮名（面区点 1-7-82〜85）
    DakutenKatakana { num: String },
}
