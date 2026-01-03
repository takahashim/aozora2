//! パーサーモジュール
//!
//! トークンからASTノードへの変換を行います。

pub mod command_parser;
pub mod reference_resolver;
pub mod ruby_parser;

use crate::node::{BlockParams, BlockType, MidashiLevel, MidashiStyle, Node, RubyDirection};
use crate::token::Token;

pub use command_parser::{parse_command, CommandResult};
pub use reference_resolver::{resolve_inline_ruby, resolve_references};
pub use ruby_parser::extract_ruby_base;

/// トークン列をノード列にパース
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
    let mut nodes = Vec::new();

    for token in tokens {
        let node = parse_token(token);
        nodes.push(node);
    }

    // 前方参照の解決
    resolve_references(&mut nodes);

    nodes
}

/// 単一のトークンをノードに変換
fn parse_token(token: &Token) -> Node {
    match token {
        Token::Text(text) => Node::Text(text.clone()),

        Token::Ruby { children } => {
            // ルビの親文字はここでは未解決
            // 後でreference_resolverで処理される
            let ruby_nodes = parse_tokens(children);
            Node::Ruby {
                children: vec![],
                ruby: ruby_nodes,
                direction: RubyDirection::Right,
            }
        }

        Token::PrefixedRuby {
            base_children,
            ruby_children,
        } => {
            let base_nodes = parse_tokens(base_children);
            let ruby_nodes = parse_tokens(ruby_children);
            Node::Ruby {
                children: base_nodes,
                ruby: ruby_nodes,
                direction: RubyDirection::Right,
            }
        }

        Token::Command { content } => parse_command_to_node(content),

        Token::Gaiji { description } => parse_gaiji_to_node(description),

        Token::Accent { children } => {
            let inner_nodes = parse_tokens(children);
            let text: String = inner_nodes.iter().map(|n| n.to_text()).collect();
            let converted = crate::accent::convert_accent(&text);
            Node::Text(converted)
        }
    }
}

/// トークン列をノード列に変換（再帰用、前方参照解決なし）
fn parse_tokens(tokens: &[Token]) -> Vec<Node> {
    tokens.iter().map(parse_token).collect()
}

/// コマンドをノードに変換
fn parse_command_to_node(content: &str) -> Node {
    use command_parser::CommandResult;

    match parse_command(content) {
        CommandResult::Style {
            target,
            connector,
            style_type,
        } => {
            // 後方参照スタイル: 「対象」に装飾
            Node::UnresolvedReference {
                target,
                spec: style_type.command_name().to_string(),
                connector,
            }
        }

        CommandResult::Midashi {
            target,
            level,
            style,
        } => {
            // 後方参照見出し: 「対象」は見出し
            // specにはスタイル情報も含める
            let spec = match (level, style) {
                (MidashiLevel::O, MidashiStyle::Dogyo) => "同行大見出し",
                (MidashiLevel::O, MidashiStyle::Mado) => "窓大見出し",
                (MidashiLevel::O, MidashiStyle::Normal) => "大見出し",
                (MidashiLevel::Naka, MidashiStyle::Dogyo) => "同行中見出し",
                (MidashiLevel::Naka, MidashiStyle::Mado) => "窓中見出し",
                (MidashiLevel::Naka, MidashiStyle::Normal) => "中見出し",
                (MidashiLevel::Ko, MidashiStyle::Dogyo) => "同行小見出し",
                (MidashiLevel::Ko, MidashiStyle::Mado) => "窓小見出し",
                (MidashiLevel::Ko, MidashiStyle::Normal) => "小見出し",
            };
            Node::UnresolvedReference {
                target,
                spec: spec.to_string(),
                connector: "は".to_string(),
            }
        }

        CommandResult::BlockStart { block_type, params } => Node::BlockStart { block_type, params },

        CommandResult::BlockEnd { block_type } => Node::BlockEnd { block_type },

        CommandResult::LineIndent { width } => Node::BlockStart {
            block_type: BlockType::Jisage,
            params: BlockParams {
                width: Some(width),
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
            alt,
            css_class: String::new(),
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
        },

        CommandResult::WarigakiStart => Node::BlockStart {
            block_type: BlockType::Warigaki,
            params: BlockParams::default(),
        },

        CommandResult::WarigakiEnd => Node::BlockEnd {
            block_type: BlockType::Warigaki,
        },

        CommandResult::LeftRuby { target, ruby } => Node::Ruby {
            children: vec![Node::text(&target)],
            ruby: vec![Node::text(&ruby)],
            direction: RubyDirection::Left,
        },

        CommandResult::Unknown(text) => Node::Note(text),
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
                block_type: BlockType::Jisage
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
