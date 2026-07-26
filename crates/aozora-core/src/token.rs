//! 青空文庫形式のトークン型定義

/// ソース行内の**文字（char）単位**の範囲 `[start, end)`（半開区間・0 起点）。
/// 位置情報として RawAST（`RawLine.nodes[i].span`＝[`Spanned`]）が保持する。byte ではなく
/// char 数なので、全角文字も1として数える（`line.chars().nth(start)` 等でそのまま使える）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// 開始 char オフセット（0 起点・含む）
    pub start: usize,
    /// 終了 char オフセット（含まない）
    pub end: usize,
}

impl Span {
    /// 範囲を作る。
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    /// 範囲の char 数。
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
    /// 空範囲か。
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// 値に、それが由来するソース行内の char 位置範囲（[`Span`]）を添えた器。
///
/// 位置情報が意味を持つのは**トップレベルのトークン列・生ノード列**だけ（入れ子の
/// ルビ内容などは行内位置を持たない）。その境界で値と位置がずれないよう、並行配列や
/// 生タプルではなく1つの値にまとめる。トークナイザ出力（`Vec<Spanned<Token>>`）と
/// RawAST の生ノード列（`RawLine.nodes: Vec<Spanned<Node>>`）で使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spanned<T> {
    /// 中身（[`Token`] や [`crate::node::Node`]）。
    pub node: T,
    /// `node` のソース行内 char 位置範囲。
    pub span: Span,
}

impl<T> Spanned<T> {
    /// 値と範囲を組にする。
    pub fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}

/// 青空文庫形式のトークン
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// 通常テキスト
    Text(String),

    /// 暗黙ルビ《...》のルビ部分
    /// 親文字は直前のTextトークンに含まれる
    Ruby {
        /// ルビ内のトークン列（通常はTextだが、外字を含む場合もある）
        children: Vec<Token>,
    },

    /// 明示ルビ ｜親文字《ルビ》
    PrefixedRuby {
        /// 親文字部分のトークン列
        base_children: Vec<Token>,
        /// ルビ部分のトークン列
        ruby_children: Vec<Token>,
    },

    /// コマンド ［＃...］
    Command {
        /// コマンド内容（デリミタ除く）
        content: String,
    },

    /// 外字 ※［＃...］（＃は任意。参照 dispatch_gaiji は ※［ だけで外字扱い）
    Gaiji {
        /// 外字説明（デリミタ・先頭＃を除く）
        /// 例: "「二の字点」、1-2-22" や "「丸印」、U+25CB"
        description: String,
        /// 元の記法に ＃（IGETA）があったか。
        /// 参照実装は `※［...］`（＃無し）を認めるが、その場合 EmbedGaiji の
        /// alt 名は空（gsub! が nil を返す挙動）、UnEmbedGaiji の注記も ＃無しの
        /// `［...］` で出る。この差を描画時に再現するためのフラグ。
        had_igeta: bool,
    },

    /// アクセント分解 〔...〕
    Accent {
        /// アクセント内のトークン列
        children: Vec<Token>,
    },
}

impl Token {
    /// テキストトークンを作成
    pub fn text(s: impl Into<String>) -> Self {
        Token::Text(s.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_text() {
        let token = Token::text("こんにちは");
        assert!(matches!(token, Token::Text(s) if s == "こんにちは"));
    }

    #[test]
    fn test_token_ruby() {
        let token = Token::Ruby {
            children: vec![Token::text("かんじ")],
        };
        assert!(matches!(token, Token::Ruby { .. }));
    }

    #[test]
    fn test_token_prefixed_ruby() {
        let token = Token::PrefixedRuby {
            base_children: vec![Token::text("東京")],
            ruby_children: vec![Token::text("とうきょう")],
        };
        assert!(matches!(token, Token::PrefixedRuby { .. }));
    }

    #[test]
    fn test_token_command() {
        let token = Token::Command {
            content: "「である」に傍点".to_string(),
        };
        assert!(matches!(token, Token::Command { .. }));
    }

    #[test]
    fn test_token_gaiji() {
        let token = Token::Gaiji {
            description: "「丸印」、U+25CB".to_string(),
            had_igeta: true,
        };
        assert!(matches!(token, Token::Gaiji { .. }));
    }

    #[test]
    fn test_token_accent() {
        let token = Token::Accent {
            children: vec![Token::text("cafe'")],
        };
        assert!(matches!(token, Token::Accent { .. }));
    }
}
