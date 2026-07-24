//! ASTノード型定義
//!
//! 構文解析の結果として生成されるノード型を定義します。

mod block;
mod midashi;
mod reference;
mod style;

pub use block::{BlockParams, BlockType};
pub use midashi::{MidashiLevel, MidashiStyle};
pub use reference::{InlineKind, RefSpec};
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

    /// 横組み（インライン）
    Yokogumi {
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

    /// フォントサイズ（大きな文字、小さな文字）
    FontSize {
        /// 内容のノード列
        children: Vec<Node>,
        /// サイズタイプ（大/小）
        size_type: FontSizeType,
        /// 段階レベル（1〜5）
        level: u32,
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
        /// パラメータ（割り注用）
        #[allow(dead_code)]
        params: BlockParams,
    },

    /// 注記（編集者注）
    Note(String),

    /// 行単位字下げ ［＃N字下げ］。
    /// 行に単独ならその行から複数行ブロックになり、テキストが同じ行にあれば
    /// 行全体をこの字下げで包む（参照実装 apply_jisage の unshift 相当）。
    LineJisage {
        /// 字下げ幅（em）
        width: u32,
    },

    /// 注記付き範囲の終了マーカー（外字を含む可能性がある）
    AnnotationEnd {
        /// 前置テキスト（「左に「」など）
        prefix: String,
        /// 注記内容のノード列（外字を含む可能性あり）
        content: Vec<Node>,
        /// 後置テキスト（「」の注記付き終わり」など）
        suffix: String,
    },

    /// 未解決の前方参照（パース〜解決の間だけ存在する中間ノード）。
    /// 解決器が対象を前方に見つけて [`RefSpec::resolve`] で最終ノードにするか、
    /// 見つからなければ `raw` をそのまま注記にする。
    UnresolvedReference {
        /// 対象テキスト
        target: String,
        /// 対象に適用する指定
        spec: RefSpec,
        /// 注記のもとの文字列。対象が前方に見つからなかったときは
        /// これをそのまま注記として出す（組み立て直すと元と変わることがある）。
        raw: String,
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

/// フォントサイズタイプ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSizeType {
    /// 大きな文字
    Dai,
    /// 小さな文字
    Sho,
}

impl FontSizeType {
    /// コマンド文字列からフォントサイズ情報を抽出
    ///
    /// 例: "１段階大きな文字" → Some((Dai, 1))
    /// 例: "２段階小さな文字" → Some((Sho, 2))
    pub fn from_command(command: &str) -> Option<(Self, u32)> {
        // "N段階大きな文字" または "N段階小さな文字" を検出
        if command.contains("大きな文字") {
            let level = extract_level(command).unwrap_or(1);
            return Some((FontSizeType::Dai, level));
        }
        if command.contains("小さな文字") {
            let level = extract_level(command).unwrap_or(1);
            return Some((FontSizeType::Sho, level));
        }
        None
    }
}

/// コマンド文字列から段階レベルを抽出
fn extract_level(command: &str) -> Option<u32> {
    // "１段階" "２段階" などを検出
    let chars: Vec<char> = command.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        // 漢数字を数値に変換
        let num = match c {
            '１' | '1' => Some(1),
            '２' | '2' => Some(2),
            '３' | '3' => Some(3),
            '４' | '4' => Some(4),
            '５' | '5' => Some(5),
            _ => None,
        };
        if let Some(n) = num {
            // 次の文字が "段階" かチェック
            if i + 2 < chars.len() && chars[i + 1] == '段' && chars[i + 2] == '階' {
                return Some(n);
            }
        }
    }
    None
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
            Node::Yokogumi { children } => children.iter().map(|n| n.to_text()).collect(),
            Node::Caption { children } => children.iter().map(|n| n.to_text()).collect(),
            Node::Warigaki { upper, lower } => {
                let u: String = upper.iter().map(|n| n.to_text()).collect();
                let l: String = lower.iter().map(|n| n.to_text()).collect();
                format!("{u}（{l}）")
            }
            Node::FontSize { children, .. } => children.iter().map(|n| n.to_text()).collect(),
            Node::Kaeriten(s) => s.clone(),
            Node::Okurigana(s) => s.clone(),
            Node::BlockStart { .. }
            | Node::BlockEnd { .. }
            | Node::Note(_)
            | Node::LineJisage { .. }
            | Node::AnnotationEnd { .. } => String::new(),
            // 未解決参照は解決器で必ず解決 or Note 化されるので通常ここには残らない。
            // 残った場合はもとの文字列で表す。
            Node::UnresolvedReference { raw, .. } => format!("［＃{raw}］"),
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
