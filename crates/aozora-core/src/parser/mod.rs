//! パーサーモジュール
//!
//! トークンからASTノードへの変換を行います。

mod block_parser;
pub mod command_parser;
mod content_parser;
mod reference_parser;
pub mod reference_resolver;
pub mod ruby_parser;
mod utils;

use crate::node::{BlockParams, BlockType, InlineKind, Node, NodeKind, RefSpec, RubyDirection};
use crate::token::{Span, Token, TokenKind};
use crate::tokenizer::{tokenize, tokenize_collecting_unclosed_accents};

pub use command_parser::{parse_command, CommandResult};
pub use reference_resolver::{resolve_inline_ruby, resolve_references};
pub use ruby_parser::extract_ruby_base;

/// RawAST（生AST）の1行分。ソース行・生ノード列・本文内での行番号を持つ。
///
/// これが RawAST の正の器（旧 `RawAst(Vec<Node>)` は撤去）。ブロックの開始/終了は
/// この段階では各行の中の平坦なマーカーノード（`BlockStart`/`BlockEnd`/`LineJisage`）
/// として存在し、前方参照も未解決。行をまたぐ対応付けと解決は後段（Lowerer＝
/// `crate::lower::lower_to_blocks`）が行い、[`crate::ast::Block`] のAozora AST木にする。
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawLine {
    /// もとのソース行（くの字点走査などで参照する）
    pub source: String,
    /// この行を忠実にパースした生ノード列（前方参照は未解決）。各ノードには行内 char
    /// 位置範囲（[`Span`]）を各Node自身が保持する。
    pub nodes: Vec<Node>,
    /// 本文（extract_body_lines 後）における 0 起点の行番号（位置情報）。
    pub line_no: usize,
}

/// 文書全体の RawAST（[`RawLine`] の列）。
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawDoc {
    /// 行の列
    pub lines: Vec<RawLine>,
}

/// 行の列を文書単位の RawAST（[`RawDoc`]）にパースする。各行を tokenize +
/// [`parse_raw_nodes`]（前方参照は未解決・ブロックは平坦マーカーのまま）。
pub fn parse_document_raw(lines: &[&str]) -> RawDoc {
    parse_document_raw_with_diagnostics(lines).0
}

/// パース時に検出できる診断（現状は同一行で閉じられなかったアクセント `〔…`）。
///
/// 変換出力には影響しない**検証用の副産物**。木に混ぜず別に返すのは、
/// Lowerer の [`crate::lower::LowerDiagnostic`] と同じ考え方——交換形式が運ぶのは
/// 木であって、診断は消費者が要るときだけ受け取ればよい。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDiagnostic {
    /// 本文 0 起点の行番号。
    pub line: usize,
    /// 位置（行内の char 範囲）。
    pub span: Span,
    /// 種類。
    pub kind: ParseDiagnosticKind,
}

/// [`ParseDiagnostic`] の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseDiagnosticKind {
    /// 同一行に対応する `〕` が無く、行末まで延長したアクセント。
    UnclosedAccent,
}

/// [`parse_document_raw`] と同じ木を作り、加えてパース時の診断を返す。
/// **RawDoc は `parse_document_raw` と完全に一致**（診断は追加返却のみ）。
pub fn parse_document_raw_with_diagnostics(lines: &[&str]) -> (RawDoc, Vec<ParseDiagnostic>) {
    let mut diagnostics = Vec::new();
    let raw_lines = lines
        .iter()
        .enumerate()
        .map(|(line_no, line)| {
            let (tokens, unclosed_accents) = tokenize_collecting_unclosed_accents(line);
            diagnostics.extend(unclosed_accents.into_iter().map(|span| ParseDiagnostic {
                line: line_no,
                span,
                kind: ParseDiagnosticKind::UnclosedAccent,
            }));
            RawLine {
                source: (*line).to_string(),
                nodes: parse_raw_nodes(&tokens),
                line_no,
            }
        })
        .collect();
    (RawDoc { lines: raw_lines }, diagnostics)
}

/// トークン列を**生ノード列**（RawAST の中身）にパースする。構文→木の忠実な変換のみで、
/// 前方参照は未解決・ブロックは平坦なマーカー（`BlockStart`/`BlockEnd`/`LineJisage`/
/// `UnresolvedReference`）のまま。文書単位の RawAST は [`RawLine`]/[`RawDoc`] が器。
pub fn parse_raw_nodes(tokens: &[Token]) -> Vec<Node> {
    let mut nodes = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let parsed = parse_token_with_context(token, &nodes, tokens, i);
        nodes.extend(parsed);
    }
    nodes
}

/// トークン列をノード列にパースし、行内で完結する前方参照を解決する
/// （[`parse_raw_nodes`] → [`resolve_references`] の簡便合成）。
///
/// # Examples
///
/// ```
/// use aozora_core::tokenizer::tokenize;
/// use aozora_core::parser::parse;
///
/// let nodes = parse(&tokenize("東京《とうきょう》"));
/// ```
pub fn parse(tokens: &[Token]) -> Vec<Node> {
    let mut nodes = parse_raw_nodes(tokens);
    resolve_references(&mut nodes);
    nodes
}

/// 割注コマンドが括弧の内側に置かれているか。行内の前後トークンを見ないと決まらない
/// ので、トークン列を持つ呼び出し元だけが埋められる。既定（子トークンの再帰など、
/// 行の文脈が無い場合）はどちらも false。
#[derive(Debug, Clone, Copy, Default)]
struct ParenContext {
    /// 直前が `（` で終わるテキストか
    open_before: bool,
    /// 直後が `）` で始まるテキストか
    close_after: bool,
}

/// 直前のノードがテキストで `（` で終わるかチェック
fn has_open_paren_before(nodes: &[Node]) -> bool {
    nodes.last().is_some_and(|node| {
        if let NodeKind::Text(s) = &node.kind {
            s.ends_with('（')
        } else {
            false
        }
    })
}

/// 直後のトークンがテキストで `）` で始まるかチェック
fn has_close_paren_after(tokens: &[Token], current_index: usize) -> bool {
    tokens.get(current_index + 1).is_some_and(|token| {
        if let TokenKind::Text(s) = &token.kind {
            s.starts_with('）')
        } else {
            false
        }
    })
}

/// コンテキスト付きでトークンをパース
fn parse_token_with_context(
    token: &Token,
    nodes: &[Node],
    tokens: &[Token],
    current_index: usize,
) -> Vec<Node> {
    let kinds = match &token.kind {
        TokenKind::Accent { children } => return apply_accent_to_nodes(parse_tokens(children)),
        TokenKind::Command { content } => {
            let paren = ParenContext {
                open_before: has_open_paren_before(nodes),
                close_after: has_close_paren_after(tokens, current_index),
            };
            vec![command_to_node_kind(parse_command(content), content, paren)]
        }
        _ => parse_token_kinds(token),
    };
    kinds
        .into_iter()
        .map(|kind| Node::new(kind, token.span))
        .collect()
}

/// 単一のトークンをノード（複数可）に変換
fn parse_token_kinds(token: &Token) -> Vec<NodeKind> {
    match &token.kind {
        TokenKind::Text(text) => vec![NodeKind::Text(text.clone())],
        TokenKind::HardBreak => vec![NodeKind::HardBreak],

        TokenKind::Ruby { children } => {
            // ルビの親文字はここでは未解決
            // 後でreference_resolverで処理される
            let ruby_nodes = parse_tokens(children);
            vec![NodeKind::Ruby {
                children: vec![],
                ruby: ruby_nodes,
                direction: RubyDirection::Right,
                keep_gaiji_notes_in_base: false,
            }]
        }

        TokenKind::PrefixedRuby {
            base_children,
            ruby_children,
        } => {
            let base_nodes = parse_tokens(base_children);
            let ruby_nodes = parse_tokens(ruby_children);
            // ｜ で明示された親文字は空でも「確定済み」。resolve_inline_ruby が
            // children.is_empty() で前方テキストを取り込むので、空の場合は空 Text を
            // 1つ置いて非空にし、参照実装同様に空親文字のルビ（<rb></rb>）にする
            // （例:「一番向｜《むか》」→ 一番向 は本文、親文字は空）。
            let base = if base_nodes.is_empty() {
                vec![Node::text(String::new(), token.span)]
            } else {
                base_nodes
            };
            vec![NodeKind::Ruby {
                children: base,
                ruby: ruby_nodes,
                direction: RubyDirection::Right,
                keep_gaiji_notes_in_base: false,
            }]
        }

        TokenKind::Command { content } => vec![command_to_node_kind(
            parse_command(content),
            content,
            ParenContext::default(),
        )],

        TokenKind::Gaiji {
            description,
            had_igeta,
        } => vec![parse_gaiji_to_node(description, *had_igeta)],

        TokenKind::Accent { .. } => {
            unreachable!("accent tokens are handled before kind conversion")
        }

        TokenKind::RubyPrefix => {
            unreachable!("RubyPrefix markers are folded away inside tokenize()")
        }
    }
}

/// アクセントブロック 〔…〕 の中身にアクセント変換を適用する。
/// テキストは e´ 等をアクセント文字（外字画像）に変換し、外字などはそのまま残す。
/// 参照実装 AccentParser はブロック内の全文字を処理するので、内側のルビ親文字
/// （例:〔｜Cafe'《カフエ》〕の Cafe'）にも再帰的に適用する。
fn apply_accent_to_nodes(nodes: Vec<Node>) -> Vec<Node> {
    use crate::accent::{parse_accent, AccentPart};
    let mut result = Vec::new();
    for Node { kind, span } in nodes {
        match kind {
            NodeKind::Text(s) => {
                let mut offset = 0;
                for part in parse_accent(&s) {
                    match part {
                        AccentPart::Text(t) => {
                            let width = t.chars().count();
                            result.push(Node::text(t, span_slice(span, offset, width)));
                            offset += width;
                        }
                        AccentPart::Accent {
                            jis_code,
                            name,
                            unicode,
                            source_width: width,
                        } => {
                            result.push(Node::new(
                                NodeKind::Accent {
                                    code: jis_code,
                                    name,
                                    unicode: Some(unicode),
                                },
                                span_slice(span, offset, width),
                            ));
                            offset += width;
                        }
                    }
                }
            }
            // ルビの親文字にも再帰的にアクセント変換を適用する。
            NodeKind::Ruby {
                children,
                ruby,
                direction,
                keep_gaiji_notes_in_base,
            } => result.push(Node::new(
                NodeKind::Ruby {
                    children: apply_accent_to_nodes(children),
                    ruby,
                    direction,
                    keep_gaiji_notes_in_base,
                },
                span,
            )),
            other => result.push(Node::new(other, span)),
        }
    }
    result
}

fn span_slice(span: Span, offset: usize, width: usize) -> Span {
    let (_, suffix) = span.split_at(offset);
    suffix.split_at(width).0
}

/// トークン列をノード列に変換（再帰用、前方参照解決なし）
fn parse_tokens(tokens: &[Token]) -> Vec<Node> {
    tokens
        .iter()
        .flat_map(|token| match &token.kind {
            TokenKind::Accent { children } => apply_accent_to_nodes(parse_tokens(children)),
            _ => parse_token_kinds(token)
                .into_iter()
                .map(|kind| Node::new(kind, token.span))
                .collect(),
        })
        .collect()
}

/// 解析済みコマンド [`CommandResult`] を [`NodeKind`] へ写像する。
///
/// ここは**機械的な写像だけ**を行う層で、命令文字列の解釈は `command_parser` 系が
/// 済ませている。`raw` は解決に失敗した参照を原文のまま注記に戻すために持ち回る。
fn command_to_node_kind(result: CommandResult, raw: &str, paren: ParenContext) -> NodeKind {
    match result {
        CommandResult::Style {
            target,
            connector: _,
            style_type,
        } => NodeKind::UnresolvedReference {
            target,
            spec: RefSpec::Style(style_type),
            raw: raw.to_string(),
        },

        // 注記形の ＜注記＞ は外字＋後続テキストを含みうるのでノード列にする。
        CommandResult::KutenGaiji {
            target,
            jis_code,
            annotation,
        } => NodeKind::UnresolvedReference {
            target,
            spec: RefSpec::EmbeddedGaiji {
                jis_code,
                annotation_ruby: annotation.map(|inner| parse(&tokenize(&inner))),
            },
            raw: raw.to_string(),
        },

        CommandResult::Midashi {
            target,
            level,
            style,
        } => NodeKind::UnresolvedReference {
            target,
            spec: RefSpec::Midashi { level, style },
            raw: raw.to_string(),
        },

        CommandResult::FontSize {
            target,
            size_type,
            level,
        } => NodeKind::UnresolvedReference {
            target,
            spec: RefSpec::FontSize { size_type, level },
            raw: raw.to_string(),
        },

        CommandResult::BlockStart { block_type, params } => {
            NodeKind::BlockStart { block_type, params }
        }

        CommandResult::BlockEnd {
            block_type,
            explicit,
        } => NodeKind::BlockEnd {
            block_type,
            params: BlockParams::default(),
            explicit_close: explicit,
        },

        CommandResult::LineIndent { width } => NodeKind::LineJisage { width },

        CommandResult::LineChitsuki { width } => NodeKind::BlockStart {
            block_type: BlockType::Chitsuki,
            params: BlockParams {
                width: if width > 0 { Some(width) } else { None },
                ..Default::default()
            },
        },

        CommandResult::Note(text) => NodeKind::Note(text),

        CommandResult::Image {
            filename,
            alt,
            is_photo,
            width,
            height,
        } => NodeKind::Img {
            filename,
            is_photo,
            alt,
            width,
            height,
        },

        CommandResult::Kaeriten(s) => NodeKind::Kaeriten(s),

        // 対象は直前の `ワ゛`〜`ヲ゛` そのもの（参照 DAKUTEN_KATAKANA_TABLE）。
        // 前方に無ければ解決器が raw のまま注記にする（参照 apply_rest_notes）。
        CommandResult::DakutenKatakana { num } => NodeKind::UnresolvedReference {
            target: Node::dakuten_katakana_char(&num).to_string(),
            spec: RefSpec::DakutenKatakana { num },
            raw: raw.to_string(),
        },

        CommandResult::Okurigana(s) => NodeKind::Okurigana(s),

        CommandResult::TcyStart => NodeKind::BlockStart {
            block_type: BlockType::Tcy,
            params: BlockParams::default(),
        },

        CommandResult::TcyEnd => NodeKind::BlockEnd {
            block_type: BlockType::Tcy,
            params: BlockParams::default(),
            explicit_close: false,
        },

        CommandResult::WarichuStart => NodeKind::BlockStart {
            block_type: BlockType::Warichu,
            params: BlockParams {
                has_open_paren: paren.open_before,
                ..Default::default()
            },
        },

        CommandResult::WarichuEnd => NodeKind::BlockEnd {
            block_type: BlockType::Warichu,
            params: BlockParams {
                has_close_paren: paren.close_after,
                ..Default::default()
            },
            explicit_close: false,
        },

        CommandResult::LeftRuby { target, ruby } => {
            // Ruby版と同様、左ルビは注記として出力（未実装機能）
            NodeKind::Note(format!("「{target}」の左に「{ruby}」のルビ"))
        }

        CommandResult::AnnotationRuby { target, annotation } => NodeKind::UnresolvedReference {
            target,
            spec: RefSpec::AnnotationRuby { annotation },
            raw: raw.to_string(),
        },

        CommandResult::InlineTcy { target } => NodeKind::UnresolvedReference {
            target,
            spec: RefSpec::Inline(InlineKind::Tcy),
            raw: raw.to_string(),
        },

        CommandResult::InlineKeigakomi { target } => NodeKind::UnresolvedReference {
            target,
            spec: RefSpec::Inline(InlineKind::Keigakomi),
            raw: raw.to_string(),
        },

        CommandResult::InlineYokogumi { target } => NodeKind::UnresolvedReference {
            target,
            spec: RefSpec::Inline(InlineKind::Yokogumi),
            raw: raw.to_string(),
        },

        CommandResult::InlineCaption { target } => NodeKind::UnresolvedReference {
            target,
            spec: RefSpec::Inline(InlineKind::Caption),
            raw: raw.to_string(),
        },

        CommandResult::InlineKaeriten { target } => NodeKind::UnresolvedReference {
            target,
            spec: RefSpec::Inline(InlineKind::Kaeriten),
            raw: raw.to_string(),
        },

        CommandResult::InlineOkurigana { target } => NodeKind::UnresolvedReference {
            target,
            spec: RefSpec::Inline(InlineKind::Okurigana),
            raw: raw.to_string(),
        },

        CommandResult::CaptionStart => NodeKind::BlockStart {
            block_type: BlockType::Caption,
            params: BlockParams::default(),
        },

        CommandResult::CaptionEnd => NodeKind::BlockEnd {
            block_type: BlockType::Caption,
            params: BlockParams::default(),
            explicit_close: false,
        },

        CommandResult::StyleStart { style_type } => NodeKind::BlockStart {
            block_type: BlockType::Style,
            params: BlockParams {
                style_type: Some(style_type),
                ..Default::default()
            },
        },

        CommandResult::StyleEnd { style_type } => NodeKind::BlockEnd {
            block_type: BlockType::Style,
            params: BlockParams {
                style_type: Some(style_type),
                ..Default::default()
            },
            explicit_close: false,
        },

        CommandResult::AnnotationRangeStart => NodeKind::BlockStart {
            block_type: BlockType::AnnotationRange,
            params: BlockParams::default(),
        },

        CommandResult::LeftAnnotationRangeStart => NodeKind::BlockStart {
            block_type: BlockType::LeftAnnotationRange,
            params: BlockParams::default(),
        },

        CommandResult::AnnotationRangeEnd { annotation } => NodeKind::BlockEnd {
            block_type: BlockType::AnnotationRange,
            params: BlockParams {
                annotation: Some(annotation),
                ..Default::default()
            },
            explicit_close: false,
        },

        CommandResult::LeftAnnotationRangeEnd { annotation } => NodeKind::BlockEnd {
            block_type: BlockType::LeftAnnotationRange,
            params: BlockParams {
                annotation: Some(annotation),
                ..Default::default()
            },
            explicit_close: false,
        },

        CommandResult::SideNote { target, annotation } => NodeKind::UnresolvedReference {
            target,
            spec: RefSpec::SideNote { annotation },
            raw: raw.to_string(),
        },

        CommandResult::Unknown(text) => NodeKind::Note(text),
    }
}

/// 外字をノードに変換
fn parse_gaiji_to_node(description: &str, had_igeta: bool) -> NodeKind {
    use crate::gaiji::{parse_gaiji, GaijiResult};

    match parse_gaiji(description) {
        GaijiResult::Unicode(s) => NodeKind::Gaiji {
            description: description.to_string(),
            unicode: Some(s),
            jis_code: None,
            had_igeta,
        },
        GaijiResult::JisConverted { jis_code, unicode } => NodeKind::Gaiji {
            description: description.to_string(),
            unicode: Some(unicode),
            jis_code: Some(jis_code),
            had_igeta,
        },
        GaijiResult::JisImage { jis_code } => NodeKind::Gaiji {
            description: description.to_string(),
            unicode: None,
            jis_code: Some(jis_code),
            had_igeta,
        },
        GaijiResult::Unconvertible => NodeKind::Gaiji {
            description: description.to_string(),
            unicode: None,
            jis_code: None,
            had_igeta,
        },
    }
}

#[cfg(test)]
mod intrinsic_span_tests {
    use super::*;
    use crate::token::Span;
    use crate::tokenizer::tokenize;

    #[test]
    fn parser_preserves_token_spans_and_ignores_them_for_equality() {
        let nodes = parse_raw_nodes(&tokenize("本文※［＃「丸印」、U+25CB］"));
        assert_eq!(nodes[0].span, Span::new(0, 2));
        assert_eq!(nodes[1].span, Span::new(2, 17));
        assert!(matches!(nodes[0].kind, NodeKind::Text(ref text) if text == "本文"));

        let left = Node::text("同じ", Span::new(0, 2));
        let right = Node::text("同じ", Span::new(10, 12));
        assert_eq!(left, right);
    }

    #[test]
    fn implicit_ruby_unions_base_and_reading_spans() {
        let mut nodes = parse_raw_nodes(&tokenize("東京《とう》"));
        resolve_references(&mut nodes);
        let ruby_node = nodes
            .iter()
            .find(|node| matches!(node.kind, NodeKind::Ruby { .. }))
            .expect("ruby node");
        assert_eq!(ruby_node.span, Span::new(0, 6));
        let NodeKind::Ruby { children, ruby, .. } = &ruby_node.kind else {
            unreachable!();
        };
        assert_eq!(children[0].span, Span::new(0, 2));
        assert_eq!(ruby[0].span, Span::new(3, 5));
        assert!(ruby_node.span.contains(children[0].span));
        assert!(ruby_node.span.contains(ruby[0].span));
    }

    #[test]
    fn accent_split_nodes_keep_their_source_slices() {
        let nodes = parse_raw_nodes(&tokenize("〔a e'〕"));
        assert_eq!(nodes.len(), 2);
        assert!(matches!(&nodes[0].kind, NodeKind::Text(text) if text == "a "));
        assert_eq!(nodes[0].span, Span::new(1, 3));
        assert!(
            matches!(&nodes[1].kind, NodeKind::Accent { unicode: Some(text), .. } if text == "é")
        );
        assert_eq!(nodes[1].span, Span::new(3, 5));
    }

    #[test]
    fn front_reference_splits_text_and_unions_the_command_span() {
        let tokens = tokenize("これは重要［＃「重要」は太字］");
        let command_span = tokens[1].span;
        let mut nodes = parse_raw_nodes(&tokens);
        resolve_references(&mut nodes);

        assert!(matches!(&nodes[0].kind, NodeKind::Text(text) if text == "これは"));
        assert_eq!(nodes[0].span, Span::new(0, 3));
        let NodeKind::Style { children, .. } = &nodes[1].kind else {
            panic!("expected a style node: {nodes:?}");
        };
        assert_eq!(children[0].span, Span::new(3, 5));
        assert_eq!(nodes[1].span, Span::new(3, command_span.end));
        assert!(nodes[1].span.contains(children[0].span));
    }

    #[test]
    fn unresolved_reference_keeps_its_command_span() {
        let tokens = tokenize("本文［＃「一致しない」は太字］");
        let command_span = tokens[1].span;
        let mut nodes = parse_raw_nodes(&tokens);
        resolve_references(&mut nodes);

        assert!(matches!(&nodes[1].kind, NodeKind::Note(_)));
        assert_eq!(nodes[1].span, command_span);
    }

    #[test]
    fn annotation_range_unions_markers_and_inherits_the_end_marker_span() {
        let tokens = tokenize("［＃注記付き］本文［＃「注記」の注記付き終わり］");
        let start_span = tokens[0].span;
        let end_span = tokens[2].span;
        let mut nodes = parse_raw_nodes(&tokens);
        resolve_references(&mut nodes);

        let NodeKind::Ruby { children, ruby, .. } = &nodes[0].kind else {
            panic!("expected annotation ruby: {nodes:?}");
        };
        assert_eq!(nodes[0].span, start_span.union(end_span));
        assert_eq!(children[0].span, Span::new(start_span.end, end_span.start));
        assert_eq!(ruby[0].span, end_span);
        assert!(nodes[0].span.contains(ruby[0].span));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::tokenize;

    #[test]
    fn test_parse_text() {
        let tokens = tokenize("こんにちは");
        let nodes = parse(&tokens);
        assert_eq!(nodes.len(), 1);
        assert!(matches!(&nodes[0].kind, NodeKind::Text(s) if s == "こんにちは"));
    }

    #[test]
    fn test_accent_preserves_inner_gaiji() {
        // アクセント〔…〕の中の外字 ※［＃…］ は NodeKind::Gaiji として保持され、
        // 記述文字列の生テキストに潰れない。従来は to_text() 平坦化で潰れていた。
        let tokens = tokenize("〔a※［＃ローマ数字19、37-下-11］e´〕");
        let nodes = parse(&tokens);
        // どこかに Gaiji ノードが1つ残っていること。
        let has_gaiji = nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Gaiji { .. }));
        assert!(
            has_gaiji,
            "アクセント内の外字が Gaiji ノードとして残っていない: {nodes:?}"
        );
        // 記述文字列が生テキストとして紛れ込んでいないこと。
        let has_raw_desc = nodes
            .iter()
            .any(|n| matches!(&n.kind, NodeKind::Text(s) if s.contains("37-下-11")));
        assert!(
            !has_raw_desc,
            "外字の記述が生テキストになっている: {nodes:?}"
        );
    }

    #[test]
    fn test_accent_applies_to_ruby_base() {
        // アクセントブロック内のプレフィックスルビ親文字（例:〔｜Cafe'《…》〕の
        // Cafe'）にもアクセント変換を適用し、e´ 等を NodeKind::Accent（外字画像）にする。
        let tokens = tokenize("〔｜a`b《ルビ》〕");
        let nodes = parse(&tokens);
        // ルビの親文字の中にアクセントノードがあること。
        let base_has_accent = nodes.iter().any(|n| match &n.kind {
            NodeKind::Ruby { children, .. } => children
                .iter()
                .any(|c| matches!(c.kind, NodeKind::Accent { .. })),
            _ => false,
        });
        assert!(
            base_has_accent,
            "ルビ親文字にアクセント変換が適用されていない: {nodes:?}"
        );
    }

    #[test]
    fn test_parse_prefixed_ruby() {
        let tokens = tokenize("｜東京《とうきょう》");
        let nodes = parse(&tokens);
        assert_eq!(nodes.len(), 1);
        if let NodeKind::Ruby {
            children,
            ruby,
            direction,
            ..
        } = &nodes[0].kind
        {
            assert!(matches!(&children[0].kind, NodeKind::Text(s) if s == "東京"));
            assert!(matches!(&ruby[0].kind, NodeKind::Text(s) if s == "とうきょう"));
            assert_eq!(*direction, RubyDirection::Right);
        } else {
            panic!("Expected Ruby node");
        }
    }

    #[test]
    fn test_prefixed_ruby_empty_base_not_filled() {
        // 一番向｜《むか》: ｜ で明示された空親文字。前方の「一番向」を取り込まず、
        // 親文字は空のまま（参照実装は <rb></rb> の空親文字ルビを作る）。
        let mut nodes = parse(&tokenize("一番向｜《むか》うにある"));
        resolve_references(&mut nodes);
        // 「一番向」は本文テキストとして残る（ルビ親文字に取り込まれない）。
        let has_text = nodes
            .iter()
            .any(|n| matches!(&n.kind, NodeKind::Text(s) if s.starts_with("一番向")));
        assert!(has_text, "一番向 が本文に残っていない: {nodes:?}");
        // 空親文字のルビがある。
        let empty_base_ruby = nodes.iter().any(|n| {
            matches!(&n.kind,
            NodeKind::Ruby { children, ruby, .. }
                if ruby.iter().any(|r| matches!(&r.kind, NodeKind::Text(s) if s == "むか"))
                    && children.iter().all(|c| matches!(&c.kind, NodeKind::Text(s) if s.is_empty())))
        });
        assert!(empty_base_ruby, "空親文字ルビになっていない: {nodes:?}");
    }

    #[test]
    fn test_parse_command_block_start() {
        let tokens = tokenize("［＃ここから2字下げ］");
        let nodes = parse(&tokens);
        assert_eq!(nodes.len(), 1);
        if let NodeKind::BlockStart { block_type, params } = &nodes[0].kind {
            assert_eq!(*block_type, BlockType::Jisage);
            assert_eq!(params.width, Some(2));
        } else {
            panic!("Expected BlockStart node");
        }
    }

    #[test]
    fn test_parse_command_block_end() {
        let tokens = tokenize("［＃ここで字下げ終わり］");
        let nodes = parse(&tokens);
        assert_eq!(nodes.len(), 1);
        assert!(matches!(
            &nodes[0].kind,
            NodeKind::BlockEnd {
                block_type: BlockType::Jisage,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_gaiji() {
        let tokens = tokenize("※［＃「丸印」、U+25CB］");
        let nodes = parse(&tokens);
        assert_eq!(nodes.len(), 1);
        if let NodeKind::Gaiji {
            description,
            unicode,
            ..
        } = &nodes[0].kind
        {
            assert!(description.contains("丸印"));
            assert_eq!(unicode.as_deref(), Some("○"));
        } else {
            panic!("Expected Gaiji node");
        }
    }
}

#[cfg(test)]
mod span_tests {
    use super::*;
    use crate::token::Span;

    /// RawLine.nodes の各Nodeが持つ char位置範囲で、元行を切り出せる。
    #[test]
    fn raw_nodes_have_char_spans() {
        let line = "あ※［＃「丸」、U+25CB］い《い》";
        let doc = parse_document_raw(&[line]);
        let rl = &doc.lines[0];
        let chars: Vec<char> = line.chars().collect();
        // 先頭ノードは Text("あ") で span [0,1)
        assert_eq!(rl.nodes[0].span, Span::new(0, 1));
        let s0: String = chars[rl.nodes[0].span.start..rl.nodes[0].span.end]
            .iter()
            .collect();
        assert_eq!(s0, "あ");
        // 2番目は外字 ※［＃…］。span はその範囲を覆う（先頭が ※）。
        let g = &rl.nodes[1].span;
        assert_eq!(chars[g.start], '※');
        // span で行を char 単位に切り出せることを確認（全 span が行内）。
        for sn in &rl.nodes {
            assert!(sn.span.end <= chars.len(), "span が行内: {:?}", sn.span);
        }
    }
}
