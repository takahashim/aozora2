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

use crate::node::{FontSizeType, MidashiLevel, MidashiStyle, NodeKind, RubyDirection, StyleType};
use crate::token::Span;

/// 文書全体の Aozora AST ＝トップレベル [`Block`] の列。
///
/// `lower_to_blocks` の返り値の別名。「文書 1 本＝ブロックの木の列」を表す型名として
/// 用いる（backend-neutral な正規モデル。旧称「中立AST」）。
pub type AozoraAst = Vec<Block>;

/// RawAST の [`crate::node::Node`] のインライン変種をAozora AST [`Inline`] に写す。
/// ブロック構造マーカー（`BlockStart`/`BlockEnd` の is_block=true・`LineJisage`・
/// `UnresolvedReference`）は None を返す（ブロック畳み込みが別途消費する）。
/// 割り注（apply_warichu）は状態を持たないインライン出力なので、`BlockStart`/
/// `BlockEnd` の Warichu だけは [`InlineKind::Warichu`] マーカーとして写す。
///
/// 純インライン内容（ブロックマーカーを含まない行）にはこれで十分。罫囲み等の
/// インライン形ブロック（is_block=false の開閉対）の入れ子は後続のブロック
/// 畳み込みで対にする（Phase B2 続き）。
pub fn inline_from_node(node: &crate::node::Node) -> Option<Inline> {
    use crate::node::BlockType;
    let out = match &node.kind {
        NodeKind::Text(s) => InlineKind::Text(s.clone()),
        NodeKind::Ruby {
            children,
            ruby,
            direction,
            keep_gaiji_notes_in_base,
        } => InlineKind::Ruby {
            base: to_inlines(children),
            ruby: to_inlines(ruby),
            direction: *direction,
            keep_gaiji_notes_in_base: *keep_gaiji_notes_in_base,
        },
        NodeKind::Style {
            children,
            style_type,
        } => InlineKind::Style {
            children: to_inlines(children),
            style_type: *style_type,
        },
        NodeKind::Midashi {
            children,
            level,
            style,
        } => InlineKind::Midashi {
            children: to_inlines(children),
            level: *level,
            style: *style,
        },
        NodeKind::Gaiji {
            description,
            unicode,
            jis_code,
            had_igeta,
        } => InlineKind::Gaiji {
            description: description.clone(),
            unicode: unicode.clone(),
            jis_code: jis_code.clone(),
            had_igeta: *had_igeta,
        },
        NodeKind::Accent {
            code,
            name,
            unicode,
        } => InlineKind::Accent {
            code: code.clone(),
            name: name.clone(),
            unicode: unicode.clone(),
        },
        NodeKind::Img {
            filename,
            alt,
            is_photo,
            width,
            height,
        } => InlineKind::Img {
            filename: filename.clone(),
            alt: alt.clone(),
            is_photo: *is_photo,
            width: *width,
            height: *height,
        },
        NodeKind::Tcy { children } => InlineKind::Tcy {
            children: to_inlines(children),
        },
        NodeKind::Keigakomi { children } => InlineKind::Keigakomi {
            children: to_inlines(children),
        },
        NodeKind::Yokogumi { children } => InlineKind::Yokogumi {
            children: to_inlines(children),
        },
        NodeKind::Caption { children } => InlineKind::Caption {
            children: to_inlines(children),
        },
        NodeKind::FontSize {
            children,
            size_type,
            level,
        } => InlineKind::FontSize {
            children: to_inlines(children),
            size_type: *size_type,
            level: *level,
        },
        NodeKind::Kaeriten(s) => InlineKind::Kaeriten(s.clone()),
        NodeKind::Okurigana(s) => InlineKind::Okurigana(s.clone()),
        NodeKind::Note(s) => InlineKind::Note(s.clone()),
        NodeKind::DakutenKatakana { num } => InlineKind::DakutenKatakana { num: num.clone() },
        NodeKind::AnnotationEnd {
            prefix,
            content,
            suffix,
        } => InlineKind::AnnotationEnd {
            prefix: prefix.clone(),
            content: to_inlines(content),
            suffix: suffix.clone(),
        },
        // 割り注は apply_warichu の状態なし出力。開閉をマーカーとして写す。
        NodeKind::BlockStart {
            block_type: BlockType::Warichu,
            params,
        } => InlineKind::Warichu {
            open: true,
            suppress_paren: params.has_open_paren,
        },
        NodeKind::BlockEnd {
            block_type: BlockType::Warichu,
            params,
            ..
        } => InlineKind::Warichu {
            open: false,
            suppress_paren: params.has_close_paren,
        },
        // NodeKind::Warichu{upper,lower} は構築箇所ゼロのデッドコード。念のため無視。
        NodeKind::Warichu { .. } => return None,
        // ブロック構造マーカー・未解決参照はインラインではない（畳み込みが消費）。
        NodeKind::BlockStart { .. }
        | NodeKind::BlockEnd { .. }
        | NodeKind::LineJisage { .. }
        | NodeKind::UnresolvedReference { .. } => return None,
    };
    Some(Inline::new(out, node.span))
}

/// 解決済みノード列をAozora ASTのインライン列に変換する。
///
/// 同一行に開閉が揃う見出しコマンド範囲 `［＃中見出し］…［＃中見出し終わり］`
/// （`BlockStart{Midashi, is_block=false}` … `BlockEnd{Midashi}`）はインライン
/// 見出しに畳む（参照実装は block stack への push/pop で同行に h4 を開閉する）。
/// それ以外のブロックマーカーは除外する（畳み込みが別途消費するか、未対応）。
pub fn to_inlines(nodes: &[crate::node::Node]) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < nodes.len() {
        // 同行に開閉が揃うインライン範囲コマンド（見出し・装飾・大小文字）を畳む。
        if let NodeKind::BlockStart { block_type, params } = &nodes[i].kind {
            if !params.is_block && is_inline_range_type(block_type) {
                if let Some(end) = find_matching_end(nodes, i, block_type) {
                    let inner = to_inlines(&nodes[i + 1..end]);
                    if let Some(wrapped) = wrap_inline_range(
                        block_type,
                        params,
                        inner,
                        span_for_nodes(&nodes[i..=end]),
                    ) {
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
                        out.push(Inline::from_range(
                            InlineKind::BlockInline {
                                kind,
                                children: inner,
                            },
                            span_for_nodes(&nodes[i..=end]),
                        ));
                        i = end + 1;
                        continue;
                    }
                }
            }
            // 行の途中で開く地付き（is_block=false の Chitsuki）は行末まで包む
            // （参照 close_inline_blocks が行末で閉じる）。行頭のものは classify_line
            // が LineWrap で処理済みなので、ここに来るのは本文の後に続くケース。
            //
            // 範囲由来だが `range_form` は立てない。参照でこれは `Tag::OnelineIndent`
            // で、`blank_type` は String（→包む）と OnelineIndent（→`:inline`）を
            // 別扱いし、**先に見つかった方**を返す。ここに来る＝先に本文があるので
            // 参照も String 側＝包む判定になり、`range_form` で中身を見る必要はない。
            if !params.is_block && *block_type == crate::node::BlockType::Chitsuki {
                let end = find_matching_end(nodes, i, block_type).unwrap_or(nodes.len());
                let inner = to_inlines(&nodes[i + 1..end.min(nodes.len())]);
                let span_end = if end < nodes.len() { end + 1 } else { end };
                out.push(Inline::new(
                    InlineKind::ChitsukiInline {
                        width: params.width.unwrap_or(0),
                        children: inner,
                    },
                    span_for_nodes(&nodes[i..span_end]),
                ));
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
    span: Span,
) -> Option<Inline> {
    use crate::node::{BlockType, FontSizeType};
    let kind = match block_type {
        BlockType::Midashi => Some(InlineKind::Midashi {
            children: inner,
            level: params.level.unwrap_or(crate::node::MidashiLevel::O),
            style: params
                .midashi_style
                .unwrap_or(crate::node::MidashiStyle::Normal),
        }),
        BlockType::Style => params.style_type.map(|style_type| InlineKind::Style {
            children: inner,
            style_type,
        }),
        BlockType::FontDai => Some(InlineKind::FontSize {
            children: inner,
            size_type: FontSizeType::Dai,
            level: params.font_size.unwrap_or(1),
        }),
        BlockType::FontSho => Some(InlineKind::FontSize {
            children: inner,
            size_type: FontSizeType::Sho,
            level: params.font_size.unwrap_or(1),
        }),
        BlockType::Yokogumi => Some(InlineKind::Yokogumi { children: inner }),
        BlockType::Tcy => Some(InlineKind::Tcy { children: inner }),
        BlockType::Keigakomi => Some(InlineKind::Keigakomi { children: inner }),
        BlockType::Caption => Some(InlineKind::Caption { children: inner }),
        BlockType::Warigaki => Some(InlineKind::Warigaki { children: inner }),
        _ => None,
    };
    kind.map(|kind| Inline::from_range(kind, span))
}

/// この行のインライン列を描画すると「ブロックのみ行」になるか（末尾が `</div>` や
/// `</hN>`＝参照が行末 `<br />` を抑制する形）。旧 `is_block_only_line`（描画済みHTML
/// 判定）の木版で、Lower 時に `Break` を確定させ、バックエンドの HTML 詮索を無くす。
/// 末尾インラインだけで決まる（is_block_only_line は行末タグを見るため）。
pub fn line_is_block_only(inlines: &[Inline]) -> bool {
    match inlines.last() {
        // 見出しは Normal のみ `</hN>`（dogyo-/mado- はインラインなので br）。
        Some(Inline {
            kind: InlineKind::Midashi { style, .. },
            ..
        }) => *style == crate::node::MidashiStyle::Normal,
        // 行末で開いた地付き（`…</div>`）。
        Some(Inline {
            kind: InlineKind::ChitsukiInline { .. },
            ..
        }) => true,
        // 同行開閉のブロック形（div で包むか、Normal 見出し）。
        Some(Inline {
            kind: InlineKind::BlockInline { kind, .. },
            ..
        }) => block_kind_is_block_only(kind),
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
    let mut depth = 0usize;
    for (offset, node) in nodes.iter().enumerate().skip(start) {
        match &node.kind {
            NodeKind::BlockStart { block_type: bt, .. } if bt == block_type => depth += 1,
            NodeKind::BlockEnd { block_type: bt, .. } if bt == block_type => {
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

fn span_for_nodes(nodes: &[crate::node::Node]) -> Span {
    let mut spans = nodes.iter().map(|node| node.span);
    let first = spans
        .next()
        .expect("an inline conversion range is never empty");
    spans.fold(first, Span::union)
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
    BurasageWrapped {
        /// 外側ぶら下げの折り返し幅（`margin-left`。None は Quirk の空幅）
        wrap_width: Option<u32>,
        /// 外側ぶら下げの字下げ幅（`text-indent` の算出に使う）
        width: Option<u32>,
    },
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

/// Aozora ASTのインライン要素。各要素が行内の絶対char spanを自前で持つ。
#[derive(Debug, Clone)]
pub struct Inline {
    /// インライン種別と内容。
    pub kind: InlineKind,
    /// ソース行内のchar位置範囲。
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

/// span は位置メタデータであり、構造比較には含めない。
impl PartialEq for Inline {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
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
        assert!(inlines
            .iter()
            .any(|i| matches!(i.kind, InlineKind::Ruby { .. })));
        assert!(inlines
            .iter()
            .any(|i| matches!(i.kind, InlineKind::Text(_))));
        assert!(inlines
            .iter()
            .any(|i| matches!(i.kind, InlineKind::Gaiji { .. })));
    }

    /// 割り注（apply_warichu）はブロックマーカーだがInlineKind::Warichuに写す。
    #[test]
    fn test_to_inlines_warichu_marker() {
        let nodes = parse(&tokenize("本文［＃割り注］注記［＃割り注終わり］"));
        let inlines = to_inlines(&nodes);
        let opens: Vec<&Inline> = inlines
            .iter()
            .filter(|i| matches!(i.kind, InlineKind::Warichu { open: true, .. }))
            .collect();
        let closes: Vec<&Inline> = inlines
            .iter()
            .filter(|i| matches!(i.kind, InlineKind::Warichu { open: false, .. }))
            .collect();
        assert_eq!(
            opens.len(),
            1,
            "割り注開きが InlineKind::Warichu にならない: {inlines:?}"
        );
        assert_eq!(
            closes.len(),
            1,
            "割り注終わりが InlineKind::Warichu にならない: {inlines:?}"
        );
        assert_eq!(opens[0].span, nodes[1].span);
        assert_eq!(closes[0].span, nodes[3].span);
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

    #[test]
    fn inline_inherits_node_spans_recursively_and_ignores_them_for_equality() {
        let nodes = parse(&tokenize("東京《とう》"));
        let inlines = to_inlines(&nodes);
        let ruby_node = nodes
            .iter()
            .find(|node| matches!(node.kind, NodeKind::Ruby { .. }))
            .expect("ruby node");
        let ruby_inline = inlines
            .iter()
            .find(|inline| matches!(inline.kind, InlineKind::Ruby { .. }))
            .expect("ruby inline");

        assert_eq!(ruby_inline.span, ruby_node.span);
        let InlineKind::Ruby { base, ruby, .. } = &ruby_inline.kind else {
            unreachable!();
        };
        assert_eq!(base[0].span, Span::new(0, 2));
        assert_eq!(ruby[0].span, Span::new(3, 5));
        assert!(ruby_inline.span.contains(base[0].span));
        assert!(ruby_inline.span.contains(ruby[0].span));
        assert_eq!(
            Inline::text("同じ", Span::new(0, 2)),
            Inline::text("同じ", Span::new(10, 12))
        );
    }

    #[test]
    fn inline_ranges_include_their_consumed_markers() {
        let source = "［＃太字］本文［＃太字終わり］";
        let nodes = parse(&tokenize(source));
        let inlines = to_inlines(&nodes);
        let style = inlines
            .iter()
            .find(|inline| matches!(inline.kind, InlineKind::Style { .. }))
            .expect("inline style");
        assert_eq!(style.span, span_for_nodes(&nodes));
        let InlineKind::Style { children, .. } = &style.kind else {
            unreachable!();
        };
        assert!(style.span.contains(children[0].span));
    }

    #[test]
    fn block_inline_and_chitsuki_include_their_consumed_source_ranges() {
        let block_nodes = parse(&tokenize(
            "前［＃ここから横組み］中［＃ここで横組み終わり］後",
        ));
        let block_inlines = to_inlines(&block_nodes);
        let block_inline = block_inlines
            .iter()
            .find(|inline| matches!(inline.kind, InlineKind::BlockInline { .. }))
            .expect("block inline");
        assert_eq!(block_inline.span, span_for_nodes(&block_nodes[1..4]));
        let InlineKind::BlockInline { children, .. } = &block_inline.kind else {
            unreachable!();
        };
        assert!(block_inline.span.contains(children[0].span));

        let chitsuki_nodes = parse(&tokenize("前［＃地付き］末"));
        let chitsuki_inlines = to_inlines(&chitsuki_nodes);
        let chitsuki = chitsuki_inlines
            .iter()
            .find(|inline| matches!(inline.kind, InlineKind::ChitsukiInline { .. }))
            .expect("chitsuki inline");
        assert_eq!(chitsuki.span, span_for_nodes(&chitsuki_nodes[1..]));
        let InlineKind::ChitsukiInline { children, .. } = &chitsuki.kind else {
            unreachable!();
        };
        assert!(chitsuki.span.contains(children[0].span));
    }

    #[test]
    fn annotation_end_preserves_its_node_and_content_spans() {
        let node = crate::node::Node::new(
            NodeKind::AnnotationEnd {
                prefix: "左に「".to_string(),
                content: vec![crate::node::Node::text("注記", Span::new(6, 8))],
                suffix: "」の注記付き終わり".to_string(),
            },
            Span::new(4, 18),
        );
        let inline = inline_from_node(&node).expect("annotation end inline");
        assert_eq!(inline.span, node.span);
        let InlineKind::AnnotationEnd { content, .. } = &inline.kind else {
            unreachable!();
        };
        assert_eq!(content[0].span, Span::new(6, 8));
        assert!(inline.span.contains(content[0].span));
    }
}
