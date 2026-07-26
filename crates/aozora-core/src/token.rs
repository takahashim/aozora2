//! 青空文庫形式のトークン型定義

/// ソース行内の**文字（char）単位**の範囲 `[start, end)`（半開区間・0 起点）。
/// Token と RawAST（`RawLine.nodes[i].span`＝[`Spanned`]）が保持する。byte ではなく
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
/// `Node` の intrinsic span 化は次段以降の対象なので、RawAST の生ノード列
/// （`RawLine.nodes`）では引き続きこの器を使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spanned<T> {
    /// 中身（現在は [`crate::node::Node`]）。
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

/// 青空文庫形式のトークン種別。
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
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

/// 青空文庫形式のトークン。全トークンがソース行内の絶対char spanを持つ。
#[derive(Debug, Clone)]
pub struct Token {
    /// トークンの種別と内容。
    pub kind: TokenKind,
    /// ソース行内のchar位置範囲。
    pub span: Span,
}

impl Token {
    /// 種別とソース位置からトークンを作成する。
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// テキストトークンを作成する。
    pub fn text(s: impl Into<String>, span: Span) -> Self {
        Self::new(TokenKind::Text(s.into()), span)
    }
}

/// span は位置メタデータであり、構造比較には含めない。
impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_text() {
        let token = Token::text("こんにちは", Span::new(0, 5));
        assert!(matches!(token.kind, TokenKind::Text(s) if s == "こんにちは"));
    }

    #[test]
    fn test_token_ruby() {
        let token = Token::new(
            TokenKind::Ruby {
                children: vec![Token::text("かんじ", Span::new(1, 4))],
            },
            Span::new(0, 5),
        );
        assert!(matches!(token.kind, TokenKind::Ruby { .. }));
    }

    #[test]
    fn test_token_prefixed_ruby() {
        let token = Token::new(
            TokenKind::PrefixedRuby {
                base_children: vec![Token::text("東京", Span::new(1, 3))],
                ruby_children: vec![Token::text("とうきょう", Span::new(4, 9))],
            },
            Span::new(0, 10),
        );
        assert!(matches!(token.kind, TokenKind::PrefixedRuby { .. }));
    }

    #[test]
    fn test_token_command() {
        let token = Token::new(
            TokenKind::Command {
                content: "「である」に傍点".to_string(),
            },
            Span::new(0, 10),
        );
        assert!(matches!(token.kind, TokenKind::Command { .. }));
    }

    #[test]
    fn test_token_gaiji() {
        let token = Token::new(
            TokenKind::Gaiji {
                description: "「丸印」、U+25CB".to_string(),
                had_igeta: true,
            },
            Span::new(0, 14),
        );
        assert!(matches!(token.kind, TokenKind::Gaiji { .. }));
    }

    #[test]
    fn test_token_accent() {
        let token = Token::new(
            TokenKind::Accent {
                children: vec![Token::text("cafe'", Span::new(1, 6))],
            },
            Span::new(0, 7),
        );
        assert!(matches!(token.kind, TokenKind::Accent { .. }));
    }

    #[test]
    fn test_token_equality_ignores_span_recursively() {
        let left = Token::new(
            TokenKind::Ruby {
                children: vec![Token::text("かな", Span::new(1, 3))],
            },
            Span::new(0, 4),
        );
        let right = Token::new(
            TokenKind::Ruby {
                children: vec![Token::text("かな", Span::new(11, 13))],
            },
            Span::new(10, 14),
        );
        assert_eq!(left, right);
    }
}
