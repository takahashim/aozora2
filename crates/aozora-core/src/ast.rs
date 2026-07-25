//! 中立AST（block ⊃ line ⊃ inline の木）
//!
//! architecture.md §4.1 の目標である、バックエンドが消費するクリーンな木。
//! RawAST（[`crate::node::Node`] の平坦マーカー列）を Lowerer が畳んで作る
//! （前方参照解決済み・ブロックは部分木・行末の改行は互換メタデータ [`Break`]）。
//!
//! まだパイプラインに接続されていない（Phase B1: 型定義、B2 途中: インライン変換
//! `to_inlines` まで）。ブロック畳み込み（Lowerer 本体）と新バックエンドは後続の
//! Phase で接続する。実行計画: docs/plan-neutral-ast.md。
//!
//! RawAST の [`crate::node::Node`] とは別型にすることで、バックエンドが
//! ソース文字列や `BlockStart`/`BlockEnd` マーカーを見られないようにする
//! （architecture.md §4.2 型の壁）。

use crate::node::{FontSizeType, MidashiLevel, MidashiStyle, RubyDirection, StyleType};

/// RawAST の [`crate::node::Node`] のインライン変種を中立AST [`Inline`] に写す。
/// ブロック構造マーカー（`BlockStart`/`BlockEnd` の is_block=true・`LineJisage`・
/// `UnresolvedReference`）は None を返す（ブロック畳み込みが別途消費する）。
/// 割り注（apply_warichu）は状態を持たないインライン出力なので、`BlockStart`/
/// `BlockEnd` の Warichu だけは [`Inline::Warichu`] マーカーとして写す。
///
/// 純インライン内容（ブロックマーカーを含まない行）にはこれで十分。罫囲み等の
/// インライン形ブロック（is_block=false の開閉対）の入れ子は後続のブロック
/// 畳み込みで対にする（Phase B2 続き）。
pub fn inline_from_node(node: &crate::node::Node) -> Option<Inline> {
    use crate::node::{BlockType, Node};
    let out = match node {
        Node::Text(s) => Inline::Text(s.clone()),
        Node::Ruby {
            children,
            ruby,
            direction,
            keep_gaiji_notes_in_base,
        } => Inline::Ruby {
            base: to_inlines(children),
            ruby: to_inlines(ruby),
            direction: *direction,
            keep_gaiji_notes_in_base: *keep_gaiji_notes_in_base,
        },
        Node::Style {
            children,
            style_type,
        } => Inline::Style {
            children: to_inlines(children),
            style_type: *style_type,
        },
        Node::Midashi {
            children,
            level,
            style,
        } => Inline::Midashi {
            children: to_inlines(children),
            level: *level,
            style: *style,
        },
        Node::Gaiji {
            description,
            unicode,
            jis_code,
            had_igeta,
        } => Inline::Gaiji {
            description: description.clone(),
            unicode: unicode.clone(),
            jis_code: jis_code.clone(),
            had_igeta: *had_igeta,
        },
        Node::Accent {
            code,
            name,
            unicode,
        } => Inline::Accent {
            code: code.clone(),
            name: name.clone(),
            unicode: unicode.clone(),
        },
        Node::Img {
            filename,
            alt,
            is_photo,
            width,
            height,
        } => Inline::Img {
            filename: filename.clone(),
            alt: alt.clone(),
            is_photo: *is_photo,
            width: *width,
            height: *height,
        },
        Node::Tcy { children } => Inline::Tcy {
            children: to_inlines(children),
        },
        Node::Keigakomi { children } => Inline::Keigakomi {
            children: to_inlines(children),
        },
        Node::Yokogumi { children } => Inline::Yokogumi {
            children: to_inlines(children),
        },
        Node::Caption { children } => Inline::Caption {
            children: to_inlines(children),
        },
        Node::FontSize {
            children,
            size_type,
            level,
        } => Inline::FontSize {
            children: to_inlines(children),
            size_type: *size_type,
            level: *level,
        },
        Node::Kaeriten(s) => Inline::Kaeriten(s.clone()),
        Node::Okurigana(s) => Inline::Okurigana(s.clone()),
        Node::Note(s) => Inline::Note(s.clone()),
        Node::DakutenKatakana { num } => Inline::DakutenKatakana { num: num.clone() },
        Node::AnnotationEnd {
            prefix,
            content,
            suffix,
        } => Inline::AnnotationEnd {
            prefix: prefix.clone(),
            content: to_inlines(content),
            suffix: suffix.clone(),
        },
        // 割り注は apply_warichu の状態なし出力。開閉をマーカーとして写す。
        Node::BlockStart {
            block_type: BlockType::Warichu,
            params,
        } => Inline::Warichu {
            open: true,
            suppress_paren: params.has_open_paren,
        },
        Node::BlockEnd {
            block_type: BlockType::Warichu,
            params,
            ..
        } => Inline::Warichu {
            open: false,
            suppress_paren: params.has_close_paren,
        },
        // Node::Warichu{upper,lower} は構築箇所ゼロのデッドコード。念のため無視。
        Node::Warichu { .. } => return None,
        // ブロック構造マーカー・未解決参照はインラインではない（畳み込みが消費）。
        Node::BlockStart { .. }
        | Node::BlockEnd { .. }
        | Node::LineJisage { .. }
        | Node::UnresolvedReference { .. } => return None,
    };
    Some(out)
}

/// 解決済みノード列を中立ASTのインライン列に変換する。
///
/// 同一行に開閉が揃う見出しコマンド範囲 `［＃中見出し］…［＃中見出し終わり］`
/// （`BlockStart{Midashi, is_block=false}` … `BlockEnd{Midashi}`）はインライン
/// 見出しに畳む（参照実装は block stack への push/pop で同行に h4 を開閉する）。
/// それ以外のブロックマーカーは除外する（畳み込みが別途消費するか、未対応）。
pub fn to_inlines(nodes: &[crate::node::Node]) -> Vec<Inline> {
    use crate::node::Node;
    let mut out = Vec::new();
    let mut i = 0;
    while i < nodes.len() {
        // 同行に開閉が揃うインライン範囲コマンド（見出し・装飾・大小文字）を畳む。
        if let Node::BlockStart { block_type, params } = &nodes[i] {
            if !params.is_block && is_inline_range_type(block_type) {
                if let Some(end) = find_matching_end(nodes, i, block_type) {
                    let inner = to_inlines(&nodes[i + 1..end]);
                    if let Some(wrapped) = wrap_inline_range(block_type, params, inner) {
                        out.push(wrapped);
                        i = end + 1;
                        continue;
                    }
                }
            }
            // 行の途中で開閉する **ブロック形**（is_block=true、例:
            // `TEXT［＃ここから横組み］…［＃ここで横組み終わり］`）は、参照が
            // ブロック開始タグ（div/h4）を行内に埋め込むので BlockInline に畳む。
            if params.is_block {
                if let Some(end) = find_matching_end(nodes, i, block_type) {
                    if let Some(kind) = crate::lower::block_kind_of(block_type, params) {
                        let inner = to_inlines(&nodes[i + 1..end]);
                        out.push(Inline::BlockInline {
                            kind,
                            children: inner,
                        });
                        i = end + 1;
                        continue;
                    }
                }
            }
            // 行の途中で開く地付き（is_block=false の Chitsuki）は行末まで包む
            // （参照 close_inline_blocks が行末で閉じる）。行頭のものは classify_line
            // が LineWrap で処理済みなので、ここに来るのは本文の後に続くケース。
            if !params.is_block && *block_type == crate::node::BlockType::Chitsuki {
                let end = find_matching_end(nodes, i, block_type).unwrap_or(nodes.len());
                let inner = to_inlines(&nodes[i + 1..end.min(nodes.len())]);
                out.push(Inline::ChitsukiInline {
                    width: params.width.unwrap_or(0),
                    children: inner,
                });
                i = if end < nodes.len() {
                    end + 1
                } else {
                    nodes.len()
                };
                continue;
            }
        }
        if let Some(inl) = inline_from_node(&nodes[i]) {
            out.push(inl);
        }
        i += 1;
    }
    out
}

/// 同行に畳めるインライン範囲コマンドの種類か（見出し・装飾・大小文字・
/// 横組み・縦中横・罫囲み・キャプション・割書）。
fn is_inline_range_type(block_type: &crate::node::BlockType) -> bool {
    use crate::node::BlockType;
    matches!(
        block_type,
        BlockType::Midashi
            | BlockType::Style
            | BlockType::FontDai
            | BlockType::FontSho
            | BlockType::Yokogumi
            | BlockType::Tcy
            | BlockType::Keigakomi
            | BlockType::Caption
            | BlockType::Warigaki
    )
}

/// インライン範囲コマンドの開閉対を対応する [`Inline`] に包む。
fn wrap_inline_range(
    block_type: &crate::node::BlockType,
    params: &crate::node::BlockParams,
    inner: Vec<Inline>,
) -> Option<Inline> {
    use crate::node::{BlockType, FontSizeType};
    match block_type {
        BlockType::Midashi => Some(Inline::Midashi {
            children: inner,
            level: params.level.unwrap_or(crate::node::MidashiLevel::O),
            style: params
                .midashi_style
                .unwrap_or(crate::node::MidashiStyle::Normal),
        }),
        BlockType::Style => params.style_type.map(|style_type| Inline::Style {
            children: inner,
            style_type,
        }),
        BlockType::FontDai => Some(Inline::FontSize {
            children: inner,
            size_type: FontSizeType::Dai,
            level: params.font_size.unwrap_or(1),
        }),
        BlockType::FontSho => Some(Inline::FontSize {
            children: inner,
            size_type: FontSizeType::Sho,
            level: params.font_size.unwrap_or(1),
        }),
        BlockType::Yokogumi => Some(Inline::Yokogumi { children: inner }),
        BlockType::Tcy => Some(Inline::Tcy { children: inner }),
        BlockType::Keigakomi => Some(Inline::Keigakomi { children: inner }),
        BlockType::Caption => Some(Inline::Caption { children: inner }),
        BlockType::Warigaki => Some(Inline::Warigaki { children: inner }),
        _ => None,
    }
}

/// この行のインライン列を描画すると「ブロックのみ行」になるか（末尾が `</div>` や
/// `</hN>`＝参照が行末 `<br />` を抑制する形）。旧 `is_block_only_line`（描画済みHTML
/// 判定）の木版で、Lower 時に `Break` を確定させ、バックエンドの HTML 詮索を無くす。
/// 末尾インラインだけで決まる（is_block_only_line は行末タグを見るため）。
pub fn line_is_block_only(inlines: &[Inline]) -> bool {
    match inlines.last() {
        // 見出しは Normal のみ `</hN>`（dogyo-/mado- はインラインなので br）。
        Some(Inline::Midashi { style, .. }) => *style == crate::node::MidashiStyle::Normal,
        // 行末で開いた地付き（`…</div>`）。
        Some(Inline::ChitsukiInline { .. }) => true,
        // 同行開閉のブロック形（div で包むか、Normal 見出し）。
        Some(Inline::BlockInline { kind, .. }) => block_kind_is_block_only(kind),
        _ => false,
    }
}

/// `BlockInline` の種類が末尾 `</div>`/`</hN>`（ブロックのみ）になるか。
fn block_kind_is_block_only(kind: &BlockKind) -> bool {
    match kind {
        BlockKind::Midashi { style, .. } => *style == crate::node::MidashiStyle::Normal,
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
        BlockKind::Burasage { .. } => false,
    }
}

/// `start` の `BlockStart` に対応する同種の `BlockEnd` の添字を返す（入れ子対応）。
fn find_matching_end(
    nodes: &[crate::node::Node],
    start: usize,
    block_type: &crate::node::BlockType,
) -> Option<usize> {
    use crate::node::Node;
    let mut depth = 0usize;
    for (offset, node) in nodes.iter().enumerate().skip(start) {
        match node {
            Node::BlockStart { block_type: bt, .. } if bt == block_type => depth += 1,
            Node::BlockEnd { block_type: bt, .. } if bt == block_type => {
                depth -= 1;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

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
    Burasage {
        /// margin-left em（折り返し字下げ幅。参照 wrap_width）。空（コンマなし）のとき
        /// None → margin-left を空文字（Quirk）にする。
        wrap_width: Option<u32>,
        /// 字下げ幅 em（空のとき None＝0 扱い）。text-indent = width - wrap_width。
        width: Option<u32>,
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
    /// 見出し（同行・窓見出し＝インライン見出し。`ここから…見出し` は BlockKind 側）
    Midashi {
        children: Vec<Inline>,
        level: MidashiLevel,
        style: MidashiStyle,
    },
    /// 左注記範囲の終了マーカー（外字を含みうる。`crate::node::Node::AnnotationEnd`）
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use crate::tokenizer::tokenize;

    /// 解決済みノード列 → Inline 列（純インライン内容）。ブロックマーカーを含まない
    /// 一般的な行はこれで写せることを固定する。
    #[test]
    fn test_to_inlines_pure_inline() {
        let nodes = parse(&tokenize("東京《とうきょう》の本文※［＃「丸印」、U+25CB］"));
        let inlines = to_inlines(&nodes);
        // ルビ・テキスト・外字がインラインとして写ること。
        assert!(inlines.iter().any(|i| matches!(i, Inline::Ruby { .. })));
        assert!(inlines.iter().any(|i| matches!(i, Inline::Text(_))));
        assert!(inlines.iter().any(|i| matches!(i, Inline::Gaiji { .. })));
    }

    /// 割り注（apply_warichu）はブロックマーカーだが開閉を Inline::Warichu に写す。
    #[test]
    fn test_to_inlines_warichu_marker() {
        let nodes = parse(&tokenize("本文［＃割り注］注記［＃割り注終わり］"));
        let inlines = to_inlines(&nodes);
        let opens = inlines
            .iter()
            .filter(|i| matches!(i, Inline::Warichu { open: true, .. }))
            .count();
        let closes = inlines
            .iter()
            .filter(|i| matches!(i, Inline::Warichu { open: false, .. }))
            .count();
        assert_eq!(
            opens, 1,
            "割り注開きが Inline::Warichu にならない: {inlines:?}"
        );
        assert_eq!(
            closes, 1,
            "割り注終わりが Inline::Warichu にならない: {inlines:?}"
        );
    }

    /// ブロック構造マーカー（ここから字下げ）はインラインに現れない。
    #[test]
    fn test_to_inlines_skips_block_markers() {
        let nodes = parse(&tokenize("［＃ここから２字下げ］"));
        let inlines = to_inlines(&nodes);
        assert!(
            inlines.is_empty(),
            "ブロックマーカーがインライン化された: {inlines:?}"
        );
    }
}
