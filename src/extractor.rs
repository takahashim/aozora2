//! プレーンテキスト抽出

use crate::gaiji::convert_gaiji;
use crate::token::Token;

/// トークン列からプレーンテキストを抽出
pub struct PlainTextExtractor;

impl PlainTextExtractor {
    /// トークン列をプレーンテキストに変換
    pub fn extract(tokens: &[Token]) -> String {
        tokens.iter().map(Self::extract_token).collect()
    }

    /// 単一トークンからテキストを抽出
    fn extract_token(token: &Token) -> String {
        match token {
            // テキスト: そのまま出力
            Token::Text(s) => s.clone(),

            // 暗黙ルビ: 削除（親文字は直前のTextに含まれる）
            Token::Ruby { .. } => String::new(),

            // 明示ルビ: 親文字部分のみ抽出
            Token::PrefixedRuby { base_children, .. } => Self::extract(base_children),

            // コマンド: 削除
            Token::Command { .. } => String::new(),

            // 外字: Unicode文字に変換
            Token::Gaiji { description } => convert_gaiji(description).to_string(),

            // アクセント: 内容を抽出
            Token::Accent { children } => Self::extract(children),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::Tokenizer;

    fn extract(input: &str) -> String {
        let mut tokenizer = Tokenizer::new(input);
        let tokens = tokenizer.tokenize();
        PlainTextExtractor::extract(&tokens)
    }

    #[test]
    fn test_plain_text() {
        assert_eq!(extract("こんにちは"), "こんにちは");
    }

    #[test]
    fn test_ruby_removed() {
        assert_eq!(extract("漢字《かんじ》"), "漢字");
    }

    #[test]
    fn test_prefixed_ruby() {
        assert_eq!(extract("｜東京《とうきょう》"), "東京");
    }

    #[test]
    fn test_command_removed() {
        assert_eq!(extract("猫である［＃「である」に傍点］"), "猫である");
    }

    #[test]
    fn test_gaiji_unicode() {
        assert_eq!(extract("※［＃「丸印」、U+25CB］"), "○");
    }

    #[test]
    fn test_complex() {
        assert_eq!(
            extract("吾輩《わがはい》は猫《ねこ》である［＃「である」に傍点］"),
            "吾輩は猫である"
        );
    }
}
