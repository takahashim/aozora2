//! ルビ親文字抽出
//!
//! テキストからルビの親文字を抽出します。
//! 青空文庫形式では、ルビ記号（《》）の直前の同一文字種別の連続を親文字として扱います。

use crate::char_type::{CharType, CharTypeExt};
use crate::node::{Node, NodeKind};

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
    if last_char_type == CharType::HankakuTerminate {
        // 参照 push_char の特例: hankaku_terminate（. ; " ? ! )）は直前が hankaku
        // なら同じグループに入る（Fig. → Fig. が親文字）。直前の hankaku 連を含める。
        if base_start > 0 && chars[base_start - 1].char_type() == CharType::Hankaku {
            let mut i = base_start - 1;
            while i > 0 && chars[i - 1].char_type() == CharType::Hankaku {
                i -= 1;
            }
            base_start = i;
        }
    } else if last_char_type != CharType::Else {
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
    if matches!(&last_node.kind, NodeKind::Note(_)) {
        let (remaining, base) = nodes.split_at(nodes.len() - 1);
        return Some((remaining.to_vec(), base.to_vec()));
    }

    // 参照実装 RubyBuffer#create_ruby は蓄積した ruby_buf 全体を親文字にする。
    // スタイル span 等のインラインタグ（char_type :else）が直前にあると、その
    // タグが（前方参照で親文字を取り込んだ状態で）そのまま親文字になる。
    // 例:「公事根源［＃「公事根源」は斜体］《くじこんげん》」→ 親文字は
    // <span class="shatai">公事根源</span>。Note と同じくタグ単独を親文字にする。
    if matches!(
        &last_node.kind,
        NodeKind::Style { .. }
            | NodeKind::Tcy { .. }
            | NodeKind::FontSize { .. }
            | NodeKind::Keigakomi { .. }
            | NodeKind::Yokogumi { .. }
            | NodeKind::Caption { .. }
    ) {
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

        match &node.kind {
            NodeKind::Text(text) => {
                // テキストノードは文字種別で分割
                if let Some(result) = extract_ruby_base(text) {
                    if result.char_type == last_char_type {
                        if !result.base.is_empty() {
                            let split = text.chars().count() - result.base.chars().count();
                            base_nodes.push(Node::text(result.base, node.span.split_at(split).1));
                        }
                        if !result.remaining.is_empty() {
                            let split = result.remaining.chars().count();
                            remaining_nodes
                                .push(Node::text(result.remaining, node.span.split_at(split).0));
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
            NodeKind::Gaiji { .. } => {
                // 外字は漢字として扱う
                if last_char_type == CharType::Kanji {
                    base_nodes.push(node.clone());
                } else {
                    found_different_type = true;
                    remaining_nodes.push(node.clone());
                }
            }
            NodeKind::Accent { .. } => {
                // アクセント付き文字は半角として扱う
                if last_char_type == CharType::Hankaku {
                    base_nodes.push(node.clone());
                } else {
                    found_different_type = true;
                    remaining_nodes.push(node.clone());
                }
            }
            NodeKind::DakutenKatakana { .. } => {
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
    use crate::token::Span;

    fn text(value: &str) -> Node {
        Node::text(value, Span::new(0, value.chars().count()))
    }

    fn node(kind: NodeKind) -> Node {
        Node::new(kind, Span::new(0, 0))
    }

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
    fn test_extract_ruby_base_hankaku_terminate_joins_hankaku() {
        // hankaku_terminate（.）は直前の hankaku 連と同じグループ（Fig. → Fig.）。
        let result = extract_ruby_base("本文Fig.").unwrap();
        assert_eq!(result.base, "Fig.");
        assert_eq!(result.remaining, "本文");
        // 直前が hankaku でなければ終端記号だけが親文字（あ. → .）。
        let result = extract_ruby_base("あ.").unwrap();
        assert_eq!(result.base, ".");
        assert_eq!(result.remaining, "あ");
        // 終端記号が2つなら最後の1つだけ（Fig.. → .）。
        let result = extract_ruby_base("Fig..").unwrap();
        assert_eq!(result.base, ".");
        assert_eq!(result.remaining, "Fig.");
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
        let nodes = vec![text("私の東京")];
        let (remaining, base) = extract_ruby_base_from_nodes(&nodes).unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(matches!(&remaining[0].kind, NodeKind::Text(s) if s == "私の"));
        assert_eq!(base.len(), 1);
        assert!(matches!(&base[0].kind, NodeKind::Text(s) if s == "東京"));
    }

    #[test]
    fn test_extract_ruby_base_from_nodes_with_gaiji() {
        let nodes = vec![
            text("私の"),
            node(NodeKind::Gaiji {
                description: "外字".to_string(),
                unicode: Some("字".to_string()),
                jis_code: None,
                had_igeta: true,
            }),
        ];
        let (remaining, base) = extract_ruby_base_from_nodes(&nodes).unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(matches!(&remaining[0].kind, NodeKind::Text(s) if s == "私の"));
        assert_eq!(base.len(), 1);
        assert!(matches!(&base[0].kind, NodeKind::Gaiji { .. }));
    }

    #[test]
    fn test_extract_ruby_base_from_nodes_with_style() {
        // 直前がスタイル span（斜体等）なら、そのタグが単独で親文字になる
        // （例:「…。公事根源［＃「公事根源」は斜体］《くじこんげん》」）。
        let nodes = vec![
            text("持っている。"),
            node(NodeKind::Style {
                children: vec![text("公事根源")],
                style_type: crate::node::StyleType::Italic,
            }),
        ];
        let (remaining, base) = extract_ruby_base_from_nodes(&nodes).unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(matches!(&remaining[0].kind, NodeKind::Text(s) if s == "持っている。"));
        assert_eq!(base.len(), 1);
        assert!(matches!(&base[0].kind, NodeKind::Style { .. }));
    }

    #[test]
    fn test_extract_ruby_base_from_nodes_kanji_gaiji() {
        let nodes = vec![
            text("東"),
            node(NodeKind::Gaiji {
                description: "京".to_string(),
                unicode: Some("京".to_string()),
                jis_code: None,
                had_igeta: true,
            }),
        ];
        let (remaining, base) = extract_ruby_base_from_nodes(&nodes).unwrap();
        assert!(remaining.is_empty());
        assert_eq!(base.len(), 2);
    }
}
