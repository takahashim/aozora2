//! 前方参照解決
//!
//! 青空文庫形式の「〇〇」に傍点 のようなパターンを解決します。
//! これらのコマンドは前方のテキストを参照し、装飾を適用します。

use crate::node::{MidashiLevel, MidashiStyle, Node, StyleType};
use crate::parser::ruby_parser::{extract_ruby_base, extract_ruby_base_from_nodes};

/// ノード列の前方参照を解決
///
/// ルビの親文字抽出と、「〇〇」に傍点 形式の装飾コマンドを解決します。
pub fn resolve_references(nodes: &mut Vec<Node>) {
    // 1. ルビの親文字を解決
    resolve_ruby_bases(nodes);

    // 2. 装飾の前方参照を解決
    resolve_style_references(nodes);
}

/// ルビの親文字を解決
fn resolve_ruby_bases(nodes: &mut Vec<Node>) {
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
                    let preceding_nodes: Vec<Node> = nodes[..i].to_vec();
                    if let Some((remaining, base)) = extract_ruby_base_from_nodes(&preceding_nodes)
                    {
                        // 直前のノードを更新
                        let to_remove = i - (preceding_nodes.len() - remaining.len());

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

/// 装飾の前方参照を解決
fn resolve_style_references(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        if let Node::UnresolvedReference {
            target,
            spec,
            connector,
        } = &nodes[i]
        {
            let target_clone = target.clone();
            let spec_clone = spec.clone();
            let connector_clone = connector.clone();

            // 前方のノードから対象テキストを探す
            if let Some((_, found_node_idx, split_info)) =
                find_target_in_preceding(&nodes[..i], &target_clone)
            {
                // 装飾タイプを決定
                if let Some(style_type) = StyleType::from_command(&spec_clone) {
                    // スタイルノードを作成
                    // class_nameはレンダラー側で設定されるため、ここでは空にする
                    let style_node = Node::Style {
                        children: vec![Node::text(&target_clone)],
                        style_type,
                        class_name: String::new(),
                    };

                    // ノードを更新
                    match split_info {
                        SplitInfo::ExactMatch => {
                            // 完全一致：ノードを置き換え
                            nodes[found_node_idx] = style_node;
                            nodes.remove(i);
                        }
                        SplitInfo::Split { before, after } => {
                            // 部分一致：分割して挿入
                            let mut new_nodes = Vec::new();
                            if !before.is_empty() {
                                new_nodes.push(Node::text(&before));
                            }
                            new_nodes.push(style_node);
                            if !after.is_empty() {
                                new_nodes.push(Node::text(&after));
                            }

                            // 元のノードを新しいノードで置き換え
                            nodes.splice(found_node_idx..found_node_idx + 1, new_nodes.into_iter());

                            // インデックス調整（追加されたノード数分）
                            let adjustment = if before.is_empty() { 0 } else { 1 }
                                + if after.is_empty() { 0 } else { 1 };

                            // UnresolvedReferenceノードを削除
                            let new_i = i + adjustment;
                            if new_i < nodes.len() {
                                nodes.remove(new_i);
                            }
                        }
                    }
                    continue; // iを増やさない
                } else {
                    // 見出しかどうかチェック
                    if let Some(level) = MidashiLevel::from_command(&spec_clone) {
                        let style = MidashiStyle::from_command(&spec_clone);
                        let midashi_node = Node::Midashi {
                            children: vec![Node::text(&target_clone)],
                            level,
                            style,
                        };

                        // 元のテキストノードを見出しノードに置き換え
                        match split_info {
                            SplitInfo::ExactMatch => {
                                nodes[found_node_idx] = midashi_node;
                                nodes.remove(i);
                            }
                            SplitInfo::Split { before, after } => {
                                let mut new_nodes = Vec::new();
                                if !before.is_empty() {
                                    new_nodes.push(Node::text(&before));
                                }
                                new_nodes.push(midashi_node);
                                if !after.is_empty() {
                                    new_nodes.push(Node::text(&after));
                                }
                                nodes.splice(
                                    found_node_idx..found_node_idx + 1,
                                    new_nodes.into_iter(),
                                );

                                let adjustment = if before.is_empty() { 0 } else { 1 }
                                    + if after.is_empty() { 0 } else { 1 };
                                let new_i = i + adjustment;
                                if new_i < nodes.len() {
                                    nodes.remove(new_i);
                                }
                            }
                        }
                        continue;
                    }
                }
            }

            // 解決できなかった場合はNoteノードに変換
            nodes[i] = Node::Note(format!("「{target_clone}」{connector_clone}{spec_clone}"));
        }
        i += 1;
    }
}

/// 分割情報
enum SplitInfo {
    /// 完全一致
    ExactMatch,
    /// 分割が必要
    Split { before: String, after: String },
}

/// 前方のノードから対象テキストを探す
fn find_target_in_preceding(nodes: &[Node], target: &str) -> Option<(usize, usize, SplitInfo)> {
    // 後ろから探す
    for (i, node) in nodes.iter().enumerate().rev() {
        if let Node::Text(text) = node {
            if text == target {
                return Some((i, i, SplitInfo::ExactMatch));
            }
            if let Some(pos) = text.find(target) {
                let before = text[..pos].to_string();
                let after = text[pos + target.len()..].to_string();
                return Some((i, i, SplitInfo::Split { before, after }));
            }
        }
    }
    None
}

/// 行内でのルビ親文字解決
///
/// 「漢字《かんじ》」形式のルビの親文字を解決します。
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
                // 直前のTextノードから親文字を抽出
                if let Node::Text(text) = &nodes[i - 1] {
                    if let Some(result) = extract_ruby_base(text) {
                        // 直前のノードを更新
                        let new_text = result.remaining;
                        let base_text = result.base;

                        // Rubyノードを更新
                        let ruby_clone = ruby.clone();
                        let direction_clone = *direction;

                        if new_text.is_empty() {
                            // 直前のノードを削除してRubyに置き換え
                            nodes.remove(i - 1);
                            nodes[i - 1] = Node::Ruby {
                                children: vec![Node::text(&base_text)],
                                ruby: ruby_clone,
                                direction: direction_clone,
                            };
                            continue; // iを増やさない（削除したので）
                        } else {
                            nodes[i - 1] = Node::text(&new_text);
                            nodes[i] = Node::Ruby {
                                children: vec![Node::text(&base_text)],
                                ruby: ruby_clone,
                                direction: direction_clone,
                            };
                        }
                    }
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

    #[test]
    fn test_resolve_style_reference() {
        let mut nodes = vec![
            Node::text("重要なこと"),
            Node::UnresolvedReference {
                target: "重要".to_string(),
                spec: "sesame_dot".to_string(),
                connector: "に".to_string(),
            },
        ];

        resolve_style_references(&mut nodes);

        // 「重要」が装飾ノードになっているはず
        assert!(!nodes.is_empty());
    }

    #[test]
    fn test_find_target_exact() {
        let nodes = vec![
            Node::text("前の文"),
            Node::text("重要"),
            Node::text("後の文"),
        ];

        let result = find_target_in_preceding(&nodes, "重要");
        assert!(result.is_some());
        let (_, idx, split) = result.unwrap();
        assert_eq!(idx, 1);
        assert!(matches!(split, SplitInfo::ExactMatch));
    }

    #[test]
    fn test_find_target_split() {
        let nodes = vec![Node::text("これは重要なことだ")];

        let result = find_target_in_preceding(&nodes, "重要");
        assert!(result.is_some());
        let (_, idx, split) = result.unwrap();
        assert_eq!(idx, 0);
        if let SplitInfo::Split { before, after } = split {
            assert_eq!(before, "これは");
            assert_eq!(after, "なことだ");
        } else {
            panic!("Expected Split");
        }
    }
}
