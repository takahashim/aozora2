//! プレーンテキスト抽出

use aozora_core::accent::convert_accent;
use aozora_core::gaiji::convert_gaiji;
use aozora_core::token::Token;

/// トークン列をプレーンテキストに変換
pub fn extract(tokens: &[Token]) -> String {
    tokens.iter().map(extract_token).collect()
}

/// 単一トークンからテキストを抽出
fn extract_token(token: &Token) -> String {
    match token {
        // テキスト: そのまま出力
        Token::Text(s) => s.clone(),

        // 暗黙ルビ: 削除（親文字は直前のTextに含まれる）
        Token::Ruby { .. } => String::new(),

        // 明示ルビ: 親文字部分のみ抽出
        Token::PrefixedRuby { base_children, .. } => extract(base_children),

        // コマンド: 削除
        Token::Command { .. } => String::new(),

        // 外字: Unicode文字列に変換
        Token::Gaiji { description } => convert_gaiji(description),

        // アクセント: 内容を抽出してアクセント変換
        Token::Accent { children } => convert_accent(&extract(children)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aozora_core::tokenizer::Tokenizer;

    fn tokenize_and_extract(input: &str) -> String {
        let mut tokenizer = Tokenizer::new(input);
        let tokens = tokenizer.tokenize();
        extract(&tokens)
    }

    #[test]
    fn test_plain_text() {
        assert_eq!(tokenize_and_extract("こんにちは"), "こんにちは");
    }

    #[test]
    fn test_ruby_removed() {
        assert_eq!(tokenize_and_extract("漢字《かんじ》"), "漢字");
    }

    #[test]
    fn test_prefixed_ruby() {
        assert_eq!(tokenize_and_extract("｜東京《とうきょう》"), "東京");
    }

    #[test]
    fn test_command_removed() {
        assert_eq!(
            tokenize_and_extract("猫である［＃「である」に傍点］"),
            "猫である"
        );
    }

    #[test]
    fn test_gaiji_unicode() {
        assert_eq!(tokenize_and_extract("※［＃「丸印」、U+25CB］"), "○");
    }

    #[test]
    fn test_complex() {
        assert_eq!(
            tokenize_and_extract("吾輩《わがはい》は猫《ねこ》である［＃「である」に傍点］"),
            "吾輩は猫である"
        );
    }

    #[test]
    fn test_gaiji_multichar() {
        // カ゚ = カ (U+30AB) + 半濁点 (U+309A)
        assert_eq!(
            tokenize_and_extract("カ゚※［＃半濁点付き片仮名カ、1-05-87］のテスト"),
            "カ゚カ゚のテスト"
        );
    }

    #[test]
    fn test_accent_conversion() {
        assert_eq!(tokenize_and_extract("〔cafe'〕"), "café");
        assert_eq!(tokenize_and_extract("〔nai:ve〕"), "naïve");
    }

    #[test]
    fn test_accent_in_sentence() {
        assert_eq!(
            tokenize_and_extract("彼は〔cafe'〕で珈琲《コーヒー》を飲んだ"),
            "彼はcaféで珈琲を飲んだ"
        );
    }
}
