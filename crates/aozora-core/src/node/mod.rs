//! ASTノード型定義
//!
//! 構文解析の結果として生成されるノード型を定義します。

mod block;
mod midashi;
mod style;

pub use block::{BlockParams, BlockType};
pub use midashi::{MidashiLevel, MidashiStyle};
pub use style::StyleType;

use crate::char_type::CharType;

/// ASTノード
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// プレーンテキスト
    Text(String),

    /// ルビ
    Ruby {
        /// 親文字のノード列
        children: Vec<Node>,
        /// ルビテキストのノード列
        ruby: Vec<Node>,
        /// ルビの方向
        direction: RubyDirection,
    },

    /// 装飾（傍点、傍線、太字など）
    Style {
        /// 装飾対象のノード列
        children: Vec<Node>,
        /// 装飾タイプ
        style_type: StyleType,
        /// CSSクラス名
        class_name: String,
    },

    /// 見出し
    Midashi {
        /// 見出しテキストのノード列
        children: Vec<Node>,
        /// 見出しレベル
        level: MidashiLevel,
        /// 見出しスタイル
        style: MidashiStyle,
    },

    /// 外字
    Gaiji {
        /// 外字説明
        description: String,
        /// Unicode文字（変換済みの場合）
        unicode: Option<String>,
        /// JISコード
        jis_code: Option<String>,
    },

    /// アクセント文字
    Accent {
        /// JISコード
        code: String,
        /// 文字名
        name: String,
        /// Unicode文字
        unicode: Option<String>,
    },

    /// 画像
    Img {
        /// ファイル名
        filename: String,
        /// 代替テキスト
        alt: String,
        /// CSSクラス
        css_class: String,
        /// 幅
        width: Option<u32>,
        /// 高さ
        height: Option<u32>,
    },

    /// 縦中横
    Tcy {
        /// 内容のノード列
        children: Vec<Node>,
    },

    /// 罫囲み
    Keigakomi {
        /// 内容のノード列
        children: Vec<Node>,
    },

    /// キャプション
    Caption {
        /// 内容のノード列
        children: Vec<Node>,
    },

    /// 割書き
    Warigaki {
        /// 上段のノード列
        upper: Vec<Node>,
        /// 下段のノード列
        lower: Vec<Node>,
    },

    /// 返り点
    Kaeriten(String),

    /// 訓点送り仮名
    Okurigana(String),

    /// ブロック開始
    BlockStart {
        /// ブロックタイプ
        block_type: BlockType,
        /// パラメータ
        params: BlockParams,
    },

    /// ブロック終了
    BlockEnd {
        /// ブロックタイプ
        block_type: BlockType,
    },

    /// 注記（編集者注）
    Note(String),

    /// 未解決の前方参照
    UnresolvedReference {
        /// 対象テキスト
        target: String,
        /// 装飾指定
        spec: String,
        /// 接続詞（に、は、の）
        connector: String,
    },

    /// 濁点カタカナ参照
    DakutenKatakana {
        /// JISコードの末尾番号
        num: String,
    },
}

/// ルビの方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RubyDirection {
    /// 通常（縦書き右、横書き上）
    #[default]
    Right,
    /// 左ルビ（縦書き左、横書き下）
    Left,
}

impl Node {
    /// テキストノードを作成
    pub fn text(s: impl Into<String>) -> Self {
        Node::Text(s.into())
    }

    /// ノードからプレーンテキストを抽出
    pub fn to_text(&self) -> String {
        match self {
            Node::Text(s) => s.clone(),
            Node::Ruby { children, .. } => children.iter().map(|n| n.to_text()).collect(),
            Node::Style { children, .. } => children.iter().map(|n| n.to_text()).collect(),
            Node::Midashi { children, .. } => children.iter().map(|n| n.to_text()).collect(),
            Node::Gaiji {
                unicode,
                description,
                ..
            } => unicode.clone().unwrap_or_else(|| description.clone()),
            Node::Accent { unicode, name, .. } => unicode.clone().unwrap_or_else(|| name.clone()),
            Node::Img { alt, .. } => alt.clone(),
            Node::Tcy { children } => children.iter().map(|n| n.to_text()).collect(),
            Node::Keigakomi { children } => children.iter().map(|n| n.to_text()).collect(),
            Node::Caption { children } => children.iter().map(|n| n.to_text()).collect(),
            Node::Warigaki { upper, lower } => {
                let u: String = upper.iter().map(|n| n.to_text()).collect();
                let l: String = lower.iter().map(|n| n.to_text()).collect();
                format!("{u}（{l}）")
            }
            Node::Kaeriten(s) => s.clone(),
            Node::Okurigana(s) => s.clone(),
            Node::BlockStart { .. } | Node::BlockEnd { .. } | Node::Note(_) => String::new(),
            Node::UnresolvedReference {
                target,
                spec,
                connector,
            } => {
                format!("［＃「{target}」{connector}{spec}］")
            }
            Node::DakutenKatakana { num } => match num.as_str() {
                "2" => "ワ゛".to_string(),
                "3" => "ヰ゛".to_string(),
                "4" => "ヱ゛".to_string(),
                "5" => "ヲ゛".to_string(),
                _ => String::new(),
            },
        }
    }

    /// ノードの最後の文字種別を取得（ルビ親文字抽出用）
    pub fn last_char_type(&self) -> Option<CharType> {
        match self {
            Node::Text(s) => s.chars().last().map(|c| {
                let ct = crate::char_type::CharType::classify(c);
                if ct.can_be_ruby_base() {
                    ct
                } else {
                    CharType::Else
                }
            }),
            Node::Gaiji { .. } => Some(CharType::Kanji),
            Node::Accent { .. } => Some(CharType::Hankaku),
            Node::DakutenKatakana { .. } => Some(CharType::Katakana),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_node() {
        let node = Node::text("こんにちは");
        assert_eq!(node.to_text(), "こんにちは");
    }

    #[test]
    fn test_ruby_node() {
        let node = Node::Ruby {
            children: vec![Node::text("漢字")],
            ruby: vec![Node::text("かんじ")],
            direction: RubyDirection::Right,
        };
        assert_eq!(node.to_text(), "漢字");
    }

    #[test]
    fn test_gaiji_node_to_text() {
        let node = Node::Gaiji {
            description: "丸印".to_string(),
            unicode: Some("○".to_string()),
            jis_code: None,
        };
        assert_eq!(node.to_text(), "○");

        let node = Node::Gaiji {
            description: "不明な文字".to_string(),
            unicode: None,
            jis_code: None,
        };
        assert_eq!(node.to_text(), "不明な文字");
    }

    #[test]
    fn test_last_char_type() {
        let node = Node::text("漢字");
        assert_eq!(node.last_char_type(), Some(CharType::Kanji));

        let node = Node::Gaiji {
            description: "外字".to_string(),
            unicode: None,
            jis_code: None,
        };
        assert_eq!(node.last_char_type(), Some(CharType::Kanji));
    }
}
