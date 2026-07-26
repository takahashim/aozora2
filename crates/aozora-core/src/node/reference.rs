//! 前方参照の指定（`「対象」に装飾` の「装飾」部分）を型で表す。
//!
//! パーサはコマンドを解析した時点で装飾の種類を型として確定できる。以前は
//! これを文字列 `spec` に符号化して [`NodeKind::UnresolvedReference`] に載せ、
//! 解決器が再びパースし直していた（型→文字列→型の往復）。[`RefSpec`] を
//! ノードに直接載せることで、その往復とマジック文字列による結合をなくす。

use super::{FontSizeType, MidashiLevel, MidashiStyle, Node, NodeKind, RubyDirection, StyleType};
use crate::token::Span;

/// 前方参照で対象に適用する指定
#[derive(Debug, Clone, PartialEq)]
pub enum RefSpec {
    /// 装飾（傍点・傍線など）
    Style(StyleType),
    /// 見出し
    Midashi {
        /// 見出しレベル
        level: MidashiLevel,
        /// 見出しスタイル（同行・窓・通常）
        style: MidashiStyle,
    },
    /// フォントサイズ
    FontSize {
        /// 大／小
        size_type: FontSizeType,
        /// 段階
        level: u32,
    },
    /// インライン要素（縦中横・罫囲み・横組み・キャプション）
    Inline(InlineKind),
    /// 注記ルビ（「対象」に「注記」の注記）
    AnnotationRuby {
        /// ルビとして表示する注記
        annotation: String,
    },
    /// 傍記（対象の各文字の脇に注記を並べる）
    SideNote {
        /// 各文字に添える注記
        annotation: String,
    },
    /// 句点コード指定による外字。2形態ある:
    /// - **置換** `「5」はローマ数字、1-13-25`: 対象「5」を外字画像に置き換える
    ///   （`annotation_ruby: None`）。
    /// - **注記** `「すはどり」に「※［＃…］鳩」の注記`: 対象を基底、外字を含む注記を
    ///   ルビにする（`annotation_ruby: Some(ルビノード列)`）。参照実装は注記形でも
    ///   基底を落として外字だけ出すバグがあるが、ここでは正しくルビにする。
    EmbeddedGaiji {
        /// 面-区-点コード（例 `1-13-25`）。置換形で使う。
        jis_code: String,
        /// 注記形のときのルビ内容（外字＋後続テキスト）。置換形は None。
        annotation_ruby: Option<Vec<Node>>,
    },
}

impl RefSpec {
    /// 対象の子ノード列にこの指定を適用して最終的なノードを作る
    pub fn resolve(&self, children: Vec<Node>, span: Span) -> Node {
        let kind = match self {
            // 置換形（annotation_ruby=None）: 対象を外字画像に置き換える（正しい）。
            // 注記形（Some）: 対象を基底・外字入り注記をルビにした Ruby を作る
            // （参照実装は基底を落とすバグだが、ここでは正しくルビにする）。
            RefSpec::EmbeddedGaiji {
                jis_code,
                annotation_ruby,
            } => match annotation_ruby {
                Some(ruby) => NodeKind::Ruby {
                    children,
                    ruby: ruby
                        .clone()
                        .into_iter()
                        .map(|node| inherit_span(node, span))
                        .collect(),
                    direction: RubyDirection::Right,
                    keep_gaiji_notes_in_base: true,
                },
                None => NodeKind::Gaiji {
                    description: String::new(),
                    unicode: None,
                    jis_code: Some(jis_code.clone()),
                    had_igeta: false,
                },
            },
            RefSpec::Style(style_type) => NodeKind::Style {
                children,
                style_type: *style_type,
            },
            RefSpec::Midashi { level, style } => NodeKind::Midashi {
                children,
                level: *level,
                style: *style,
            },
            RefSpec::FontSize { size_type, level } => NodeKind::FontSize {
                children,
                size_type: *size_type,
                level: *level,
            },
            RefSpec::Inline(inline_kind) => return inline_kind.create_node(children, span),
            RefSpec::AnnotationRuby { annotation } => NodeKind::Ruby {
                children,
                ruby: vec![Node::text(annotation, span)],
                direction: RubyDirection::Right,
                keep_gaiji_notes_in_base: true,
            },
            RefSpec::SideNote { annotation } => {
                // 親文字の文字数だけ注記を繰り返し、&nbsp; で区切る
                let char_count: usize = children.iter().map(|n| n.to_text().chars().count()).sum();
                let repeated: String = std::iter::repeat(annotation.as_str())
                    .take(char_count.max(1))
                    .collect::<Vec<_>>()
                    .join("\u{00a0}");
                NodeKind::Ruby {
                    children,
                    ruby: vec![Node::text(&repeated, span)],
                    direction: RubyDirection::Right,
                    keep_gaiji_notes_in_base: true,
                }
            }
        };
        Node::new(kind, span)
    }
}

/// インライン要素の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineKind {
    /// 縦中横
    Tcy,
    /// 罫囲み
    Keigakomi,
    /// 横組み
    Yokogumi,
    /// キャプション
    Caption,
    /// 返り点（前方参照「対象」は返り点）
    Kaeriten,
    /// 訓点送り仮名（前方参照「対象」は訓点送り仮名）
    Okurigana,
}

impl InlineKind {
    fn create_node(self, children: Vec<Node>, span: Span) -> Node {
        let kind = match self {
            InlineKind::Tcy => NodeKind::Tcy { children },
            InlineKind::Keigakomi => NodeKind::Keigakomi { children },
            InlineKind::Yokogumi => NodeKind::Yokogumi { children },
            InlineKind::Caption => NodeKind::Caption { children },
            // 返り点・送り仮名は対象テキストを平文にして sub/sup で包む。
            InlineKind::Kaeriten => {
                NodeKind::Kaeriten(children.iter().map(|n| n.to_text()).collect())
            }
            InlineKind::Okurigana => {
                NodeKind::Okurigana(children.iter().map(|n| n.to_text()).collect())
            }
        };
        Node::new(kind, span)
    }
}

fn inherit_span(mut node: Node, span: Span) -> Node {
    node.span = span;
    match &mut node.kind {
        NodeKind::Ruby { children, ruby, .. } => {
            for child in children.iter_mut().chain(ruby) {
                *child = inherit_span(child.clone(), span);
            }
        }
        NodeKind::Style { children, .. }
        | NodeKind::Midashi { children, .. }
        | NodeKind::Tcy { children }
        | NodeKind::Keigakomi { children }
        | NodeKind::Yokogumi { children }
        | NodeKind::Caption { children }
        | NodeKind::FontSize { children, .. } => {
            for child in children {
                *child = inherit_span(child.clone(), span);
            }
        }
        NodeKind::Warichu { upper, lower } => {
            for child in upper.iter_mut().chain(lower) {
                *child = inherit_span(child.clone(), span);
            }
        }
        NodeKind::AnnotationEnd { content, .. } => {
            for child in content {
                *child = inherit_span(child.clone(), span);
            }
        }
        _ => {}
    }
    node
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_annotation_ruby_inherits_the_reference_span() {
        let span = Span::new(3, 18);
        let node = RefSpec::AnnotationRuby {
            annotation: "ちゅうき".to_string(),
        }
        .resolve(vec![Node::text("対象", Span::new(3, 5))], span);

        let NodeKind::Ruby { children, ruby, .. } = node.kind else {
            panic!("expected ruby");
        };
        assert_eq!(node.span, span);
        assert_eq!(children[0].span, Span::new(3, 5));
        assert_eq!(ruby[0].span, span);
    }

    #[test]
    fn generated_side_note_and_embedded_gaiji_inherit_the_reference_span() {
        let span = Span::new(4, 22);
        let side_note = RefSpec::SideNote {
            annotation: "注".to_string(),
        }
        .resolve(vec![Node::text("対象", Span::new(4, 6))], span);
        let NodeKind::Ruby { ruby, .. } = side_note.kind else {
            panic!("expected side-note ruby");
        };
        assert_eq!(side_note.span, span);
        assert_eq!(ruby[0].span, span);

        let embedded = RefSpec::EmbeddedGaiji {
            jis_code: "1-13-25".to_string(),
            annotation_ruby: Some(vec![Node::text("外字", Span::new(0, 2))]),
        }
        .resolve(vec![Node::text("対象", Span::new(4, 6))], span);
        let NodeKind::Ruby { ruby, .. } = embedded.kind else {
            panic!("expected embedded-gaiji ruby");
        };
        assert_eq!(embedded.span, span);
        assert_eq!(ruby[0].span, span);
    }
}
