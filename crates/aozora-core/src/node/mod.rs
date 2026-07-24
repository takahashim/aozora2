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
        /// 写真か（false なら挿絵）。CSSクラスはレンダラが決める。
        is_photo: bool,
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
        /// 割り注・装飾など、終了タグ生成に必要なパラメータ
        params: BlockParams,
        /// ［＃ここで…終わり］形式（CLOSE_MARK）で閉じたか。
        /// この形式は参照実装 exec_block_end_command で @terprip=false を立て、
        /// その行の行末 <br /> を抑制する。bare ［＃…終わり］は false。
        explicit_close: bool,
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

/// コマンド文字列から段階レベルを抽出（"N段階" の N）。
///
/// 参照実装 PAT_CHARSIZE は convert_japanese_number 後に `(\d*)段階` を取るので、
/// 全角/半角数字も漢数字も段階の直前の値を読む。従来は 1〜5 の全半角数字しか
/// 拾えず「６段階大きな文字」等が既定の 1 になっていた（dai1）。0〜9 と漢数字
/// 一〜十を「段階」直前の1文字として拾う。
fn extract_level(command: &str) -> Option<u32> {
    let chars: Vec<char> = command.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let num = match c {
            '０'..='９' => Some(*c as u32 - '０' as u32),
            '0'..='9' => Some(*c as u32 - '0' as u32),
            '一' => Some(1),
            '二' => Some(2),
            '三' => Some(3),
            '四' => Some(4),
            '五' => Some(5),
            '六' => Some(6),
            '七' => Some(7),
            '八' => Some(8),
            '九' => Some(9),
            '十' => Some(10),
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

    /// 濁点付き片仮名（面区点 1-7-82〜85）の表示文字。
    /// 参照実装 aozora2html の DAKUTEN_KATAKANA_TABLE 相当（唯一の定義）。
    pub fn dakuten_katakana_char(num: &str) -> &'static str {
        match num {
            "2" => "ワ゛",
            "3" => "ヰ゛",
            "4" => "ヱ゛",
            "5" => "ヲ゛",
            _ => "",
        }
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
            Node::DakutenKatakana { num } => Node::dakuten_katakana_char(num).to_string(),
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
    fn test_font_size_level_beyond_five() {
        // 1〜5 だけでなく 6〜9 の全角数字も段階レベルとして拾う（従来は既定 1 に落ちた）。
        assert_eq!(
            FontSizeType::from_command("６段階大きな文字"),
            Some((FontSizeType::Dai, 6))
        );
        assert_eq!(
            FontSizeType::from_command("九段階小さな文字"),
            Some((FontSizeType::Sho, 9))
        );
        assert_eq!(
            FontSizeType::from_command("2段階大きな文字"),
            Some((FontSizeType::Dai, 2))
        );
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
