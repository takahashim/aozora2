//! ルビ親文字解決
//!
//! 「漢字《かんじ》」形式のルビの親文字を解決します。

use crate::node::Node;
use crate::parser::ruby_parser::extract_ruby_base_from_nodes;

/// ノード列のルビ親文字を解決
pub fn resolve_ruby_bases(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        // 親文字が空のRubyノードを探す
        if let Node::Ruby {
            children,
            ruby,
            direction: _,
        } = &nodes[i]
        {
            if children.is_empty() && !ruby.is_empty() {
                // 直前のノードから親文字を抽出
                if i > 0 {
                    let preceding_len = i;
                    if let Some((remaining, base)) = extract_ruby_base_from_nodes(&nodes[..i]) {
                        // 直前のノードを更新
                        let to_remove = i - (preceding_len - remaining.len());

                        // 残りのノードで前半を置き換え
                        nodes.splice(..i, remaining.into_iter());

                        // 新しいインデックスを計算
                        let new_i = nodes.len() - (nodes.len() - to_remove);

                        // Rubyノードを更新
                        if let Some(Node::Ruby { children: c, .. }) = nodes.get_mut(new_i) {
                            *c = base;
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

/// 行内でのルビ親文字解決
///
/// 「漢字《かんじ》」形式のルビの親文字を解決します。
/// 外字ノードも漢字として親文字に含めます。
pub fn resolve_inline_ruby(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        if let Node::Ruby {
            children,
            ruby,
            direction,
        } = &nodes[i]
        {
            if children.is_empty() && !ruby.is_empty() && i > 0 {
                let ruby_clone = ruby.clone();
                let direction_clone = *direction;
                let preceding_len = i;

                // 直前のノード列から親文字を抽出（外字も含む）
                if let Some((remaining, base)) = extract_ruby_base_from_nodes(&nodes[..i]) {
                    // 残りのノード数を計算
                    let nodes_to_remove = preceding_len - remaining.len();

                    // 前半を残りのノードで置き換え
                    let start_idx = i - nodes_to_remove;
                    nodes.splice(start_idx..i, std::iter::empty());

                    // 新しいインデックスを計算
                    let new_i = start_idx;

                    // 前半部分を挿入
                    nodes.splice(..new_i, remaining.into_iter());

                    // Rubyノードを更新（インデックスが変わっているので再計算）
                    let ruby_idx = nodes
                        .iter()
                        .position(|n| matches!(n, Node::Ruby { children: c, .. } if c.is_empty()));

                    if let Some(idx) = ruby_idx {
                        nodes[idx] = Node::Ruby {
                            children: base,
                            ruby: ruby_clone,
                            direction: direction_clone,
                        };
                    }
                    continue; // iを増やさない（ノードを操作したので）
                }
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::RubyDirection;

    #[test]
    fn test_resolve_inline_ruby() {
        let mut nodes = vec![
            Node::text("私の東京"),
            Node::Ruby {
                children: vec![],
                ruby: vec![Node::text("とうきょう")],
                direction: RubyDirection::Right,
            },
        ];

        resolve_inline_ruby(&mut nodes);

        assert_eq!(nodes.len(), 2);
        assert!(matches!(&nodes[0], Node::Text(s) if s == "私の"));
        if let Node::Ruby { children, ruby, .. } = &nodes[1] {
            assert!(matches!(&children[0], Node::Text(s) if s == "東京"));
            assert!(matches!(&ruby[0], Node::Text(s) if s == "とうきょう"));
        } else {
            panic!("Expected Ruby node");
        }
    }

    #[test]
    fn test_resolve_inline_ruby_full_match() {
        let mut nodes = vec![
            Node::text("東京"),
            Node::Ruby {
                children: vec![],
                ruby: vec![Node::text("とうきょう")],
                direction: RubyDirection::Right,
            },
        ];

        resolve_inline_ruby(&mut nodes);

        assert_eq!(nodes.len(), 1);
        if let Node::Ruby { children, ruby, .. } = &nodes[0] {
            assert!(matches!(&children[0], Node::Text(s) if s == "東京"));
            assert!(matches!(&ruby[0], Node::Text(s) if s == "とうきょう"));
        } else {
            panic!("Expected Ruby node");
        }
    }
}
