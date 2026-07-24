//! 青空文庫形式のトークン型定義

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
