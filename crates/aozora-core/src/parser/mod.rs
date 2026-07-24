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
use crate::token::Token;
use crate::tokenizer::tokenize;

pub use command_parser::{parse_command, CommandResult};
pub use reference_resolver::{resolve_inline_ruby, resolve_references};
pub use ruby_parser::extract_ruby_base;

/// RawAST の1行分。ソース行と、その行を忠実にパースした生ノード列を持つ。
///
/// ブロックの開始/終了は、この段階では各行の中の平坦なマーカーノード
/// （`BlockStart`/`BlockEnd`/`LineJisage`）として存在する。行をまたぐ対応付けは
/// 後段（Lowerer）が行う。
#[derive(Debug, Clone, PartialEq)]
pub struct RawLine {
    /// もとのソース行（くの字点走査などで参照する）
    pub source: String,
    /// この行を忠実にパースした生ノード列（前方参照は未解決）
    pub nodes: Vec<Node>,
}

/// 文書全体の RawAST（RawLine の列）。
#[derive(Debug, Clone, PartialEq)]
pub struct RawDoc {
    /// 行の列
    pub lines: Vec<RawLine>,
}

/// 行の列を文書単位の RawAST にパースする（各行を tokenize + parse_raw）。
pub fn parse_document_raw(lines: &[&str]) -> RawDoc {
    let raw_lines = lines
        .iter()
        .map(|line| RawLine {
            source: (*line).to_string(),
            nodes: parse_raw(&tokenize(line)).into_nodes(),
        })
        .collect();
    RawDoc { lines: raw_lines }
}

/// パーサが出力する RawAST。
///
/// 青空文庫記法を忠実に写した段階で、前方参照は未解決、ブロックは平坦な
/// マーカーのまま。[`lower`] で中立AST [`Ast`] に変換する。
///
/// （現段階では中身は依然 [`Vec<Node>`] で、raw と ast の違いは root 型のみ。
///  今後 raw専用ノードと block/line/inline の木へ段階的に分ける。）
#[derive(Debug, Clone, PartialEq)]
pub struct RawAst(Vec<Node>);

/// Lowerer が RawAST を変換した、レンダラが消費する中立AST。
#[derive(Debug, Clone, PartialEq)]
pub struct Ast(Vec<Node>);

impl RawAst {
    /// 中身のノード列を借用する
    pub fn nodes(&self) -> &[Node] {
        &self.0
    }
    /// 中身のノード列を取り出す
    pub fn into_nodes(self) -> Vec<Node> {
        self.0
    }
}

impl Ast {
    /// 中身のノード列を借用する
    pub fn nodes(&self) -> &[Node] {
        &self.0
    }
    /// 中身のノード列を取り出す
    pub fn into_nodes(self) -> Vec<Node> {
        self.0
    }
}

/// トークン列を RawAST にパースする（構文→木の忠実な変換のみ。前方参照は未解決）。
pub fn parse_raw(tokens: &[Token]) -> RawAst {
    let mut nodes = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        let parsed = parse_token_with_context(token, &nodes, tokens, i);
        nodes.extend(parsed);
    }
    RawAst(nodes)
}

/// RawAST を中立AST に lower する（前方参照の解決など）。
pub fn lower(raw: RawAst) -> Ast {
    let mut nodes = raw.into_nodes();
    resolve_references(&mut nodes);
    Ast(nodes)
}

/// トークン列をノード列にパースする（`parse_raw` → `lower` の合成の簡便版）。
///
/// # Examples
///
/// ```
/// use aozora_core::tokenizer::tokenize;
/// use aozora_core::parser::parse;
/// use aozora_core::node::Node;
///
/// let tokens = tokenize("東京《とうきょう》");
/// let nodes = parse(&tokens);
/// ```
pub fn parse(tokens: &[Token]) -> Vec<Node> {
    lower(parse_raw(tokens)).into_nodes()
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
            }]
        }

        Token::PrefixedRuby {
            base_children,
            ruby_children,
        } => {
            let base_nodes = parse_tokens(base_children);
            let ruby_nodes = parse_tokens(ruby_children);
            vec![Node::Ruby {
                children: base_nodes,
                ruby: ruby_nodes,
                direction: RubyDirection::Right,
            }]
        }

        Token::Command { content } => vec![parse_command_to_node(content)],

        Token::Gaiji { description } => vec![parse_gaiji_to_node(description)],

        Token::Accent { children } => {
            // アクセント内の子ノードを描画する。従来は全子ノードを to_text() で
            // 平坦化してから parse_accent していたため、内側の外字（※［＃…］）が
            // 記述文字列に潰れて生テキストとして出ていた（例:〔…au ※［＃ローマ数字
            // 19、37-下-11］e siècle〕）。テキストノードだけにアクセント変換を掛け、
            // 外字などテキスト以外のノードはそのまま残す。アクセント列（e＋アクセント）
            // は同一テキストノード内に収まるので分割して処理しても取りこぼさない。
            use crate::accent::{parse_accent, AccentPart};
            let mut result = Vec::new();
            for node in parse_tokens(children) {
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
                    other => result.push(other),
                }
            }
            result
        }
    }
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
        } => Node::UnresolvedReference {
            target,
            // KutenGaiji は句点コードが取れたときだけ作られるので必ず Some
            spec: RefSpec::EmbeddedGaiji {
                jis_code: utils::parse_kuten_gaiji(&spec).unwrap_or_default(),
            },
            raw: content.to_string(),
        },

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

        CommandResult::WarigakiStart => Node::BlockStart {
            block_type: BlockType::Warigaki,
            params: BlockParams::default(),
        },

        CommandResult::WarigakiEnd => Node::BlockEnd {
            block_type: BlockType::Warigaki,
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
        CommandResult::WarigakiStart => {
            let mut params = BlockParams::default();
            params.has_open_paren = has_open_paren_before(nodes);
            Node::BlockStart {
                block_type: BlockType::Warigaki,
                params,
            }
        }

        CommandResult::WarigakiEnd => {
            let mut params = BlockParams::default();
            params.has_close_paren = has_close_paren_after(tokens, current_index);
            Node::BlockEnd {
                block_type: BlockType::Warigaki,
                params,
                explicit_close: false,
            }
        }

        // その他のコマンドは通常の処理
        _ => parse_command_to_node(content),
    }
}

/// 外字をノードに変換
fn parse_gaiji_to_node(description: &str) -> Node {
    use crate::gaiji::{parse_gaiji, GaijiResult};

    match parse_gaiji(description) {
        GaijiResult::Unicode(s) => Node::Gaiji {
            description: description.to_string(),
            unicode: Some(s),
            jis_code: None,
        },
        GaijiResult::JisConverted { jis_code, unicode } => Node::Gaiji {
            description: description.to_string(),
            unicode: Some(unicode),
            jis_code: Some(jis_code),
        },
        GaijiResult::JisImage { jis_code } => Node::Gaiji {
            description: description.to_string(),
            unicode: None,
            jis_code: Some(jis_code),
        },
        GaijiResult::Unconvertible => Node::Gaiji {
            description: description.to_string(),
            unicode: None,
            jis_code: None,
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
        assert!(has_gaiji, "アクセント内の外字が Gaiji ノードとして残っていない: {nodes:?}");
        // 記述文字列が生テキストとして紛れ込んでいないこと。
        let has_raw_desc = nodes
            .iter()
            .any(|n| matches!(n, Node::Text(s) if s.contains("37-下-11")));
        assert!(!has_raw_desc, "外字の記述が生テキストになっている: {nodes:?}");
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
