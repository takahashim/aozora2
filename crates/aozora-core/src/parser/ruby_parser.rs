//! ルビ親文字抽出
//!
//! テキストからルビの親文字を抽出します。
//! 青空文庫形式では、ルビ記号（《》）の直前の同一文字種別の連続を親文字として扱います。

use crate::char_type::{CharType, CharTypeExt};
use crate::node::Node;

/// ルビ親文字の抽出結果
#[derive(Debug, Clone, PartialEq)]
pub struct RubyBaseResult {
    /// 親文字部分
    pub base: String,
    /// 残りの部分（親文字より前）
    pub remaining: String,
    /// 親文字の文字種別
    pub char_type: CharType,
}

/// テキストからルビ親文字を抽出
///
/// 後ろから同じ文字種別の連続を取得します。
///
/// # Examples
///
/// ```
/// use aozora_core::parser::ruby_parser::extract_ruby_base;
///
/// let result = extract_ruby_base("私の東京");
/// assert!(result.is_some());
/// let r = result.unwrap();
/// assert_eq!(r.base, "東京");
/// assert_eq!(r.remaining, "私の");
/// ```
pub fn extract_ruby_base(text: &str) -> Option<RubyBaseResult> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return None;
    }

    // 最後の文字の種別を取得
    let last_char = *chars.last()?;
    let last_char_type = last_char.char_type();

    // 後ろから同じ種別の文字を探す。
    // 参照実装 RubyBuffer#push_char は文字種 :else の文字を毎回フラッシュして
    // 1 文字ずつ独立させるので、:else のときは直前の 1 文字だけを親文字にする。
    let mut base_start = chars.len() - 1;
    if last_char_type != CharType::Else {
        for i in (0..chars.len()).rev() {
            if chars[i].char_type() == last_char_type {
                base_start = i;
            } else {
                break;
            }
        }
    }

    let remaining: String = chars[..base_start].iter().collect();
    let base: String = chars[base_start..].iter().collect();

    Some(RubyBaseResult {
        base,
        remaining,
        char_type: last_char_type,
    })
}

/// ノード列からルビ親文字を抽出
///
/// ノード列の最後から、親文字になりうるノードを抽出します。
/// Textノードの場合は文字種別で分割し、Gaijiノードは漢字として扱います。
pub fn extract_ruby_base_from_nodes(nodes: &[Node]) -> Option<(Vec<Node>, Vec<Node>)> {
    if nodes.is_empty() {
        return None;
    }

    // 最後のノードから文字種別を取得
    let last_node = nodes.last()?;

    // 参照実装 aozora2html では、注記のようなタグが来た時点で溜めていた親文字が
    // 確定し（RubyBuffer#push_char が文字種 :else で dump_into する）、
    // タグ自身が新しい親文字になる。直前が注記なら注記だけが親文字。
    if matches!(last_node, Node::Note(_)) {
        let (remaining, base) = nodes.split_at(nodes.len() - 1);
        return Some((remaining.to_vec(), base.to_vec()));
    }

    let last_char_type = last_node.last_char_type()?;

    let mut base_nodes = Vec::new();
    let mut remaining_nodes = Vec::new();
    let mut found_different_type = false;

    // 後ろからノードを走査
    for node in nodes.iter().rev() {
        if found_different_type {
            remaining_nodes.push(node.clone());
            continue;
        }

        match node {
            Node::Text(text) => {
                // テキストノードは文字種別で分割
                if let Some(result) = extract_ruby_base(text) {
                    if result.char_type == last_char_type {
                        if !result.base.is_empty() {
                            base_nodes.push(Node::Text(result.base));
                        }
                        if !result.remaining.is_empty() {
                            remaining_nodes.push(Node::Text(result.remaining));
                            found_different_type = true;
                        }
                    } else {
                        found_different_type = true;
                        remaining_nodes.push(node.clone());
                    }
                } else {
                    found_different_type = true;
                    remaining_nodes.push(node.clone());
                }
            }
            Node::Gaiji { .. } => {
                // 外字は漢字として扱う
                if last_char_type == CharType::Kanji {
                    base_nodes.push(node.clone());
                } else {
                    found_different_type = true;
                    remaining_nodes.push(node.clone());
                }
            }
            Node::Accent { .. } => {
                // アクセント付き文字は半角として扱う
                if last_char_type == CharType::Hankaku {
                    base_nodes.push(node.clone());
                } else {
                    found_different_type = true;
                    remaining_nodes.push(node.clone());
                }
            }
            Node::DakutenKatakana { .. } => {
                // 濁点カタカナはカタカナとして扱う
                if last_char_type == CharType::Katakana {
                    base_nodes.push(node.clone());
                } else {
                    found_different_type = true;
                    remaining_nodes.push(node.clone());
                }
            }
            _ => {
                // その他のノードは親文字にならない
                found_different_type = true;
                remaining_nodes.push(node.clone());
            }
        }
    }

    // 逆順を戻す
    base_nodes.reverse();
    remaining_nodes.reverse();

    if base_nodes.is_empty() {
        None
    } else {
        Some((remaining_nodes, base_nodes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_ruby_base_kanji() {
        let result = extract_ruby_base("東京").unwrap();
        assert_eq!(result.base, "東京");
        assert_eq!(result.remaining, "");
        assert_eq!(result.char_type, CharType::Kanji);
    }

    #[test]
    fn test_extract_ruby_base_mixed() {
        let result = extract_ruby_base("私の東京").unwrap();
        assert_eq!(result.base, "東京");
        assert_eq!(result.remaining, "私の");
        assert_eq!(result.char_type, CharType::Kanji);
    }

    #[test]
    fn test_extract_ruby_base_hiragana() {
        let result = extract_ruby_base("あいう").unwrap();
        assert_eq!(result.base, "あいう");
        assert_eq!(result.remaining, "");
        assert_eq!(result.char_type, CharType::Hiragana);
    }

    #[test]
    fn test_extract_ruby_base_katakana() {
        let result = extract_ruby_base("アイウ").unwrap();
        assert_eq!(result.base, "アイウ");
        assert_eq!(result.remaining, "");
        assert_eq!(result.char_type, CharType::Katakana);
    }

    #[test]
    fn test_extract_ruby_base_mixed_kana() {
        let result = extract_ruby_base("ひらがなカタカナ").unwrap();
        assert_eq!(result.base, "カタカナ");
        assert_eq!(result.remaining, "ひらがな");
        assert_eq!(result.char_type, CharType::Katakana);
    }

    /// 参照実装では文字種 :else の文字（。○ など）も 1 文字だけ親文字になる
    #[test]
    fn test_ruby_base_else_char_is_single() {
        let result = extract_ruby_base("テスト。").unwrap();
        assert_eq!(result.base, "。");
        assert_eq!(result.remaining, "テスト");
        assert_eq!(result.char_type, CharType::Else);

        let result = extract_ruby_base("ふう○").unwrap();
        assert_eq!(result.base, "○");
        assert_eq!(result.remaining, "ふう");
    }

    #[test]
    fn test_extract_ruby_base_empty() {
        let result = extract_ruby_base("");
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_ruby_base_from_nodes_simple() {
        let nodes = vec![Node::text("私の東京")];
        let (remaining, base) = extract_ruby_base_from_nodes(&nodes).unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(matches!(&remaining[0], Node::Text(s) if s == "私の"));
        assert_eq!(base.len(), 1);
        assert!(matches!(&base[0], Node::Text(s) if s == "東京"));
    }

    #[test]
    fn test_extract_ruby_base_from_nodes_with_gaiji() {
        let nodes = vec![
            Node::text("私の"),
            Node::Gaiji {
                description: "外字".to_string(),
                unicode: Some("字".to_string()),
                jis_code: None,
            },
        ];
        let (remaining, base) = extract_ruby_base_from_nodes(&nodes).unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(matches!(&remaining[0], Node::Text(s) if s == "私の"));
        assert_eq!(base.len(), 1);
        assert!(matches!(&base[0], Node::Gaiji { .. }));
    }

    #[test]
    fn test_extract_ruby_base_from_nodes_kanji_gaiji() {
        let nodes = vec![
            Node::text("東"),
            Node::Gaiji {
                description: "京".to_string(),
                unicode: Some("京".to_string()),
                jis_code: None,
            },
        ];
        let (remaining, base) = extract_ruby_base_from_nodes(&nodes).unwrap();
        assert!(remaining.is_empty());
        assert_eq!(base.len(), 2);
    }
}
