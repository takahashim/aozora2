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

use crate::node::{BlockParams, BlockType, InlineKind, Node, RefSpec, RubyDirection};
use crate::token::{Span, Token};
use crate::tokenizer::{tokenize, tokenize_spanned};

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
pub struct RawLine {
    /// もとのソース行（くの字点走査などで参照する）
    pub source: String,
    /// この行を忠実にパースした生ノード列（前方参照は未解決）
    pub nodes: Vec<Node>,
    /// 本文（extract_body_lines 後）における 0 起点の行番号（位置情報）。
    pub line_no: usize,
    /// 各生ノードの char 位置範囲（[`Span`]、行内の char オフセット）。`nodes[i]` に
    /// `spans[i]` が対応。ソース忠実な位置情報の置き場（Aozora AST は派生的に line 番号を持つ）。
    pub spans: Vec<Span>,
}

/// 文書全体の RawAST（[`RawLine`] の列）。
#[derive(Debug, Clone, PartialEq)]
pub struct RawDoc {
    /// 行の列
    pub lines: Vec<RawLine>,
}

/// 行の列を文書単位の RawAST（[`RawDoc`]）にパースする。各行を tokenize +
/// [`parse_raw_nodes`]（前方参照は未解決・ブロックは平坦マーカーのまま）。
pub fn parse_document_raw(lines: &[&str]) -> RawDoc {
    let raw_lines = lines
        .iter()
        .enumerate()
        .map(|(line_no, line)| {
            let (nodes, spans) = parse_raw_nodes_spanned(&tokenize_spanned(line));
            RawLine {
                source: (*line).to_string(),
                nodes,
                line_no,
                spans,
            }
        })
        .collect();
    RawDoc { lines: raw_lines }
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

/// `parse_raw_nodes` の span 付き版。各生ノードに、由来トークンの char 位置範囲
/// （[`Span`]）を対応付けて返す（`nodes[i]` と `spans[i]` が対応）。1トークンが複数
/// ノードに展開される場合、それらは同じトークン span を共有する。
pub fn parse_raw_nodes_spanned(spanned: &[(Token, Span)]) -> (Vec<Node>, Vec<Span>) {
    let tokens: Vec<Token> = spanned.iter().map(|(t, _)| t.clone()).collect();
    let mut nodes = Vec::new();
    let mut spans = Vec::new();
    for (i, (token, span)) in spanned.iter().enumerate() {
        let parsed = parse_token_with_context(token, &nodes, &tokens, i);
        for n in parsed {
            nodes.push(n);
            spans.push(*span);
        }
    }
    (nodes, spans)
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

/// 直前のノードがテキストで `（` で終わるかチェック
fn has_open_paren_before(nodes: &[Node]) -> bool {
    nodes.last().map_or(false, |node| {
        if let Node::Text(s) = node {
            s.ends_with('（')
        } else {
            false
        }
    })
}

/// 直後のトークンがテキストで `）` で始まるかチェック
fn has_close_paren_after(tokens: &[Token], current_index: usize) -> bool {
    tokens.get(current_index + 1).map_or(false, |token| {
        if let Token::Text(s) = token {
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
    match token {
        Token::Command { content } => {
            vec![parse_command_to_node_with_context(
                content,
                nodes,
                tokens,
                current_index,
            )]
        }
        _ => parse_token(token),
    }
}

/// 単一のトークンをノード（複数可）に変換
fn parse_token(token: &Token) -> Vec<Node> {
    match token {
        Token::Text(text) => vec![Node::Text(text.clone())],

        Token::Ruby { children } => {
            // ルビの親文字はここでは未解決
            // 後でreference_resolverで処理される
            let ruby_nodes = parse_tokens(children);
            vec![Node::Ruby {
                children: vec![],
                ruby: ruby_nodes,
                direction: RubyDirection::Right,
                keep_gaiji_notes_in_base: false,
            }]
        }

        Token::PrefixedRuby {
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
                vec![Node::Text(String::new())]
            } else {
                base_nodes
            };
            vec![Node::Ruby {
                children: base,
                ruby: ruby_nodes,
                direction: RubyDirection::Right,
                keep_gaiji_notes_in_base: false,
            }]
        }

        Token::Command { content } => vec![parse_command_to_node(content)],

        Token::Gaiji {
            description,
            had_igeta,
        } => vec![parse_gaiji_to_node(description, *had_igeta)],

        Token::Accent { children } => {
            // アクセント内の子ノードを描画する。従来は全子ノードを to_text() で
            // 平坦化してから parse_accent していたため、内側の外字（※［＃…］）が
            // 記述文字列に潰れて生テキストとして出ていた（例:〔…au ※［＃ローマ数字
            // 19、37-下-11］e siècle〕）。テキストノードだけにアクセント変換を掛け、
            // 外字などテキスト以外のノードはそのまま残す。アクセント列（e＋アクセント）
            // は同一テキストノード内に収まるので分割して処理しても取りこぼさない。
            apply_accent_to_nodes(parse_tokens(children))
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
    for node in nodes {
        match node {
            Node::Text(s) => {
                for part in parse_accent(&s) {
                    match part {
                        AccentPart::Text(t) => result.push(Node::Text(t)),
                        AccentPart::Accent {
                            jis_code,
                            name,
                            unicode,
                        } => result.push(Node::Accent {
                            code: jis_code,
                            name,
                            unicode: Some(unicode),
                        }),
                    }
                }
            }
            // ルビの親文字にも再帰的にアクセント変換を適用する。
            Node::Ruby {
                children,
                ruby,
                direction,
                keep_gaiji_notes_in_base,
            } => result.push(Node::Ruby {
                children: apply_accent_to_nodes(children),
                ruby,
                direction,
                keep_gaiji_notes_in_base,
            }),
            other => result.push(other),
        }
    }
    result
}

/// トークン列をノード列に変換（再帰用、前方参照解決なし）
fn parse_tokens(tokens: &[Token]) -> Vec<Node> {
    tokens.iter().flat_map(parse_token).collect()
}

/// コマンドをノードに変換
fn parse_command_to_node(content: &str) -> Node {
    use command_parser::CommandResult;

    match parse_command(content) {
        CommandResult::Style {
            target,
            connector: _,
            style_type,
        } => Node::UnresolvedReference {
            target,
            spec: RefSpec::Style(style_type),
            raw: content.to_string(),
        },

        CommandResult::KutenGaiji {
            target,
            connector: _,
            spec,
        } => {
            // 注記形 `「対象」に「<注記>」の注記` は、<注記>（外字＋後続テキスト）を
            // パースしてルビにする。置換形 `「5」はローマ数字、1-13-25` は None。
            let annotation_ruby = spec
                .strip_suffix("の注記")
                .and_then(|s| s.strip_prefix('「'))
                .and_then(|s| s.strip_suffix('」'))
                .map(|inner| parse(&tokenize(inner)));
            Node::UnresolvedReference {
                target,
                // KutenGaiji は句点コードが取れたときだけ作られるので必ず Some
                spec: RefSpec::EmbeddedGaiji {
                    jis_code: utils::parse_kuten_gaiji(&spec).unwrap_or_default(),
                    annotation_ruby,
                },
                raw: content.to_string(),
            }
        }

        CommandResult::Midashi {
            target,
            level,
            style,
        } => Node::UnresolvedReference {
            target,
            spec: RefSpec::Midashi { level, style },
            raw: content.to_string(),
        },

        CommandResult::FontSize {
            target,
            size_type,
            level,
        } => Node::UnresolvedReference {
            target,
            spec: RefSpec::FontSize { size_type, level },
            raw: content.to_string(),
        },

        CommandResult::BlockStart { block_type, params } => Node::BlockStart { block_type, params },

        CommandResult::BlockEnd {
            block_type,
            explicit,
        } => Node::BlockEnd {
            block_type,
            params: BlockParams::default(),
            explicit_close: explicit,
        },

        CommandResult::LineIndent { width } => Node::LineJisage { width },

        CommandResult::LineChitsuki { width } => Node::BlockStart {
            block_type: BlockType::Chitsuki,
            params: BlockParams {
                width: if width > 0 { Some(width) } else { None },
                ..Default::default()
            },
        },

        CommandResult::Note(text) => Node::Note(text),

        CommandResult::Image {
            filename,
            alt,
            width,
            height,
        } => Node::Img {
            filename,
            // 参照実装 exec_img_command は説明に「写真」が入っていれば写真扱い。
            // CSSクラス名の選択はレンダラに委ねる。
            is_photo: alt.contains("写真"),
            alt,
            width,
            height,
        },

        CommandResult::Kaeriten(s) => Node::Kaeriten(s),

        CommandResult::Okurigana(s) => Node::Okurigana(s),

        CommandResult::TcyStart => Node::BlockStart {
            block_type: BlockType::Tcy,
            params: BlockParams::default(),
        },

        CommandResult::TcyEnd => Node::BlockEnd {
            block_type: BlockType::Tcy,
            params: BlockParams::default(),
            explicit_close: false,
        },

        CommandResult::WarichuStart => Node::BlockStart {
            block_type: BlockType::Warichu,
            params: BlockParams::default(),
        },

        CommandResult::WarichuEnd => Node::BlockEnd {
            block_type: BlockType::Warichu,
            params: BlockParams::default(),
            explicit_close: false,
        },

        CommandResult::LeftRuby { target, ruby } => {
            // Ruby版と同様、左ルビは注記として出力（未実装機能）
            Node::Note(format!("「{target}」の左に「{ruby}」のルビ"))
        }

        CommandResult::AnnotationRuby { target, annotation } => Node::UnresolvedReference {
            target,
            spec: RefSpec::AnnotationRuby { annotation },
            raw: content.to_string(),
        },

        CommandResult::InlineTcy { target } => Node::UnresolvedReference {
            target,
            spec: RefSpec::Inline(InlineKind::Tcy),
            raw: content.to_string(),
        },

        CommandResult::InlineKeigakomi { target } => Node::UnresolvedReference {
            target,
            spec: RefSpec::Inline(InlineKind::Keigakomi),
            raw: content.to_string(),
        },

        CommandResult::InlineYokogumi { target } => Node::UnresolvedReference {
            target,
            spec: RefSpec::Inline(InlineKind::Yokogumi),
            raw: content.to_string(),
        },

        CommandResult::InlineCaption { target } => Node::UnresolvedReference {
            target,
            spec: RefSpec::Inline(InlineKind::Caption),
            raw: content.to_string(),
        },

        CommandResult::InlineKaeriten { target } => Node::UnresolvedReference {
            target,
            spec: RefSpec::Inline(InlineKind::Kaeriten),
            raw: content.to_string(),
        },

        CommandResult::InlineOkurigana { target } => Node::UnresolvedReference {
            target,
            spec: RefSpec::Inline(InlineKind::Okurigana),
            raw: content.to_string(),
        },

        CommandResult::CaptionStart => Node::BlockStart {
            block_type: BlockType::Caption,
            params: BlockParams::default(),
        },

        CommandResult::CaptionEnd => Node::BlockEnd {
            block_type: BlockType::Caption,
            params: BlockParams::default(),
            explicit_close: false,
        },

        CommandResult::StyleStart { style_type } => Node::BlockStart {
            block_type: BlockType::Style,
            params: BlockParams {
                style_type: Some(style_type),
                ..Default::default()
            },
        },

        CommandResult::StyleEnd { style_type } => Node::BlockEnd {
            block_type: BlockType::Style,
            params: BlockParams {
                style_type: Some(style_type),
                ..Default::default()
            },
            explicit_close: false,
        },

        CommandResult::AnnotationRangeStart => Node::BlockStart {
            block_type: BlockType::AnnotationRange,
            params: BlockParams::default(),
        },

        CommandResult::LeftAnnotationRangeStart => Node::BlockStart {
            block_type: BlockType::LeftAnnotationRange,
            params: BlockParams::default(),
        },

        CommandResult::AnnotationRangeEnd { annotation } => Node::BlockEnd {
            block_type: BlockType::AnnotationRange,
            params: BlockParams {
                annotation: Some(annotation),
                ..Default::default()
            },
            explicit_close: false,
        },

        CommandResult::LeftAnnotationRangeEnd { annotation } => Node::BlockEnd {
            block_type: BlockType::LeftAnnotationRange,
            params: BlockParams {
                annotation: Some(annotation),
                ..Default::default()
            },
            explicit_close: false,
        },

        CommandResult::SideNote { target, annotation } => Node::UnresolvedReference {
            target,
            spec: RefSpec::SideNote { annotation },
            raw: content.to_string(),
        },

        CommandResult::Unknown(text) => Node::Note(text),
    }
}

/// コマンドをノードに変換（コンテキスト付き）
fn parse_command_to_node_with_context(
    content: &str,
    nodes: &[Node],
    tokens: &[Token],
    current_index: usize,
) -> Node {
    use command_parser::CommandResult;

    match parse_command(content) {
        CommandResult::WarichuStart => {
            let mut params = BlockParams::default();
            params.has_open_paren = has_open_paren_before(nodes);
            Node::BlockStart {
                block_type: BlockType::Warichu,
                params,
            }
        }

        CommandResult::WarichuEnd => {
            let mut params = BlockParams::default();
            params.has_close_paren = has_close_paren_after(tokens, current_index);
            Node::BlockEnd {
                block_type: BlockType::Warichu,
                params,
                explicit_close: false,
            }
        }

        // その他のコマンドは通常の処理
        _ => parse_command_to_node(content),
    }
}

/// 外字をノードに変換
fn parse_gaiji_to_node(description: &str, had_igeta: bool) -> Node {
    use crate::gaiji::{parse_gaiji, GaijiResult};

    match parse_gaiji(description) {
        GaijiResult::Unicode(s) => Node::Gaiji {
            description: description.to_string(),
            unicode: Some(s),
            jis_code: None,
            had_igeta,
        },
        GaijiResult::JisConverted { jis_code, unicode } => Node::Gaiji {
            description: description.to_string(),
            unicode: Some(unicode),
            jis_code: Some(jis_code),
            had_igeta,
        },
        GaijiResult::JisImage { jis_code } => Node::Gaiji {
            description: description.to_string(),
            unicode: None,
            jis_code: Some(jis_code),
            had_igeta,
        },
        GaijiResult::Unconvertible => Node::Gaiji {
            description: description.to_string(),
            unicode: None,
            jis_code: None,
            had_igeta,
        },
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
        assert!(matches!(&nodes[0], Node::Text(s) if s == "こんにちは"));
    }

    #[test]
    fn test_accent_preserves_inner_gaiji() {
        // アクセント〔…〕の中の外字 ※［＃…］ は Node::Gaiji として保持され、
        // 記述文字列の生テキストに潰れない。従来は to_text() 平坦化で潰れていた。
        let tokens = tokenize("〔a※［＃ローマ数字19、37-下-11］e´〕");
        let nodes = parse(&tokens);
        // どこかに Gaiji ノードが1つ残っていること。
        let has_gaiji = nodes.iter().any(|n| matches!(n, Node::Gaiji { .. }));
        assert!(
            has_gaiji,
            "アクセント内の外字が Gaiji ノードとして残っていない: {nodes:?}"
        );
        // 記述文字列が生テキストとして紛れ込んでいないこと。
        let has_raw_desc = nodes
            .iter()
            .any(|n| matches!(n, Node::Text(s) if s.contains("37-下-11")));
        assert!(
            !has_raw_desc,
            "外字の記述が生テキストになっている: {nodes:?}"
        );
    }

    #[test]
    fn test_accent_applies_to_ruby_base() {
        // アクセントブロック内のプレフィックスルビ親文字（例:〔｜Cafe'《…》〕の
        // Cafe'）にもアクセント変換を適用し、e´ 等を Node::Accent（外字画像）にする。
        let tokens = tokenize("〔｜a`b《ルビ》〕");
        let nodes = parse(&tokens);
        // ルビの親文字の中にアクセントノードがあること。
        let base_has_accent = nodes.iter().any(|n| match n {
            Node::Ruby { children, .. } => {
                children.iter().any(|c| matches!(c, Node::Accent { .. }))
            }
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
        if let Node::Ruby {
            children,
            ruby,
            direction,
            ..
        } = &nodes[0]
        {
            assert!(matches!(&children[0], Node::Text(s) if s == "東京"));
            assert!(matches!(&ruby[0], Node::Text(s) if s == "とうきょう"));
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
            .any(|n| matches!(n, Node::Text(s) if s.starts_with("一番向")));
        assert!(has_text, "一番向 が本文に残っていない: {nodes:?}");
        // 空親文字のルビがある。
        let empty_base_ruby = nodes.iter().any(|n| {
            matches!(n,
            Node::Ruby { children, ruby, .. }
                if ruby.iter().any(|r| matches!(r, Node::Text(s) if s == "むか"))
                    && children.iter().all(|c| matches!(c, Node::Text(s) if s.is_empty())))
        });
        assert!(empty_base_ruby, "空親文字ルビになっていない: {nodes:?}");
    }

    #[test]
    fn test_parse_command_block_start() {
        let tokens = tokenize("［＃ここから2字下げ］");
        let nodes = parse(&tokens);
        assert_eq!(nodes.len(), 1);
        if let Node::BlockStart { block_type, params } = &nodes[0] {
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
            &nodes[0],
            Node::BlockEnd {
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
        if let Node::Gaiji {
            description,
            unicode,
            ..
        } = &nodes[0]
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

    /// RawLine.spans が各生ノードの char 位置範囲を正しく表す（行を char で切り出せる）。
    #[test]
    fn raw_nodes_have_char_spans() {
        let line = "あ※［＃「丸」、U+25CB］い《い》";
        let doc = parse_document_raw(&[line]);
        let rl = &doc.lines[0];
        assert_eq!(rl.nodes.len(), rl.spans.len(), "nodes と spans は同数");
        let chars: Vec<char> = line.chars().collect();
        // 先頭ノードは Text("あ") で span [0,1)
        assert_eq!(rl.spans[0], Span::new(0, 1));
        let s0: String = chars[rl.spans[0].start..rl.spans[0].end].iter().collect();
        assert_eq!(s0, "あ");
        // 2番目は外字 ※［＃…］。span はその範囲を覆う（先頭が ※）。
        let g = &rl.spans[1];
        assert_eq!(chars[g.start], '※');
        // span で行を char 単位に切り出せることを確認（全 span が行内）。
        for sp in &rl.spans {
            assert!(sp.end <= chars.len(), "span が行内: {sp:?}");
        }
    }
}
