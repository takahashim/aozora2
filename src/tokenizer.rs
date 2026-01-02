//! 青空文庫形式の字句解析（トークナイザ）

use crate::token::delimiters::*;
use crate::token::Token;

/// アクセント記号一覧
const ACCENT_MARKS: &[char] = &['\'', '`', '^', '~', ':', '&', '_', ',', '/', '@'];

/// 1行をトークン列に変換するトークナイザ
pub struct Tokenizer {
    /// 入力をcharとして保持
    chars: Vec<char>,
    /// 現在のchar位置
    pos: usize,
}

impl Tokenizer {
    /// 新しいトークナイザを作成
    pub fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    /// 入力をトークン列に変換
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();

        while self.pos < self.chars.len() {
            let ch = self.chars[self.pos];

            match ch {
                // コマンド ［＃...］ または外字 ※［＃...］の一部
                COMMAND_BEGIN => {
                    if self.peek_nth(1) == Some(IGETA) {
                        tokens.push(self.read_command());
                    } else {
                        // ［ だけならテキスト
                        tokens.push(Token::Text(ch.to_string()));
                        self.pos += 1;
                    }
                }

                // ルビ 《...》
                RUBY_BEGIN => {
                    tokens.push(self.read_ruby());
                }

                // 明示ルビ ｜...《...》
                RUBY_PREFIX => {
                    tokens.push(self.read_prefixed_ruby());
                }

                // 外字 ※［＃...］
                GAIJI_MARK => {
                    if self.peek_nth(1) == Some(COMMAND_BEGIN)
                        && self.peek_nth(2) == Some(IGETA)
                    {
                        tokens.push(self.read_gaiji());
                    } else {
                        // ※ だけならテキスト
                        tokens.push(Token::Text(ch.to_string()));
                        self.pos += 1;
                    }
                }

                // アクセント 〔...〕
                ACCENT_BEGIN => {
                    if let Some(token) = self.try_read_accent() {
                        tokens.push(token);
                    } else {
                        // アクセント記号がなければテキスト
                        tokens.push(Token::Text(ch.to_string()));
                        self.pos += 1;
                    }
                }

                // その他はテキスト
                _ => {
                    tokens.push(self.read_text());
                }
            }
        }

        tokens
    }

    /// 現在位置から n 文字先を覗く
    fn peek_nth(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    /// テキストトークンを読む（デリミタまで）
    fn read_text(&mut self) -> Token {
        let start = self.pos;

        while self.pos < self.chars.len() {
            let ch = self.chars[self.pos];

            // デリミタに遭遇したら終了
            if matches!(
                ch,
                COMMAND_BEGIN | RUBY_BEGIN | RUBY_PREFIX | GAIJI_MARK | ACCENT_BEGIN
            ) {
                break;
            }

            self.pos += 1;
        }

        let text: String = self.chars[start..self.pos].iter().collect();
        Token::Text(text)
    }

    /// コマンドトークンを読む ［＃...］
    /// ネストに対応（括弧の深さを追跡）
    fn read_command(&mut self) -> Token {
        // ［＃ をスキップ
        self.pos += 2;
        let start = self.pos;
        let mut depth = 1;

        while self.pos < self.chars.len() && depth > 0 {
            let ch = self.chars[self.pos];

            if ch == COMMAND_BEGIN {
                depth += 1;
            } else if ch == COMMAND_END {
                depth -= 1;
            }

            if depth > 0 {
                self.pos += 1;
            }
        }

        let content: String = self.chars[start..self.pos].iter().collect();

        // ］ をスキップ
        if self.pos < self.chars.len() && self.chars[self.pos] == COMMAND_END {
            self.pos += 1;
        }

        Token::Command { content }
    }

    /// ルビトークンを読む 《...》
    fn read_ruby(&mut self) -> Token {
        // 《 をスキップ
        self.pos += 1;
        let start = self.pos;

        // 》 を探す
        while self.pos < self.chars.len() && self.chars[self.pos] != RUBY_END {
            self.pos += 1;
        }

        let content: String = self.chars[start..self.pos].iter().collect();

        // 》 をスキップ
        if self.pos < self.chars.len() && self.chars[self.pos] == RUBY_END {
            self.pos += 1;
        }

        // ルビ内を再帰的にトークナイズ
        let mut inner_tokenizer = Tokenizer::new(&content);
        let children = inner_tokenizer.tokenize();

        Token::Ruby { children }
    }

    /// 明示ルビトークンを読む ｜...《...》
    fn read_prefixed_ruby(&mut self) -> Token {
        // ｜ をスキップ
        self.pos += 1;
        let base_start = self.pos;

        // 《 を探す
        while self.pos < self.chars.len() && self.chars[self.pos] != RUBY_BEGIN {
            self.pos += 1;
        }

        // 《 が見つからなかった場合
        if self.pos >= self.chars.len() {
            // ｜ をテキストとして返す（巻き戻し）
            self.pos = base_start;
            return Token::Text(RUBY_PREFIX.to_string());
        }

        let base_content: String = self.chars[base_start..self.pos].iter().collect();

        // 《 をスキップ
        self.pos += 1;
        let ruby_start = self.pos;

        // 》 を探す
        while self.pos < self.chars.len() && self.chars[self.pos] != RUBY_END {
            self.pos += 1;
        }

        let ruby_content: String = self.chars[ruby_start..self.pos].iter().collect();

        // 》 をスキップ
        if self.pos < self.chars.len() && self.chars[self.pos] == RUBY_END {
            self.pos += 1;
        }

        // 親文字とルビを再帰的にトークナイズ
        let mut base_tokenizer = Tokenizer::new(&base_content);
        let base_children = base_tokenizer.tokenize();

        let mut ruby_tokenizer = Tokenizer::new(&ruby_content);
        let ruby_children = ruby_tokenizer.tokenize();

        Token::PrefixedRuby {
            base_children,
            ruby_children,
        }
    }

    /// 外字トークンを読む ※［＃...］
    fn read_gaiji(&mut self) -> Token {
        // ※［＃ をスキップ
        self.pos += 3;
        let start = self.pos;
        let mut depth = 1;

        while self.pos < self.chars.len() && depth > 0 {
            let ch = self.chars[self.pos];

            if ch == COMMAND_BEGIN {
                depth += 1;
            } else if ch == COMMAND_END {
                depth -= 1;
            }

            if depth > 0 {
                self.pos += 1;
            }
        }

        let description: String = self.chars[start..self.pos].iter().collect();

        // ］ をスキップ
        if self.pos < self.chars.len() && self.chars[self.pos] == COMMAND_END {
            self.pos += 1;
        }

        Token::Gaiji { description }
    }

    /// アクセントトークンを試行的に読む 〔...〕
    /// アクセント記号がなければNone（テキストとして扱う）
    fn try_read_accent(&mut self) -> Option<Token> {
        let start = self.pos;

        // 〔 をスキップ
        self.pos += 1;
        let content_start = self.pos;

        // 〕 を探す
        while self.pos < self.chars.len() && self.chars[self.pos] != ACCENT_END {
            self.pos += 1;
        }

        // 〕 が見つからなかった場合
        if self.pos >= self.chars.len() {
            self.pos = start;
            return None;
        }

        let content: String = self.chars[content_start..self.pos].iter().collect();

        // アクセント記号を含むか判定
        if !Self::contains_accent_marks(&content) {
            self.pos = start;
            return None;
        }

        // 〕 をスキップ
        self.pos += 1;

        // 内容を再帰的にトークナイズ
        let mut inner_tokenizer = Tokenizer::new(&content);
        let children = inner_tokenizer.tokenize();

        Some(Token::Accent { children })
    }

    /// 文字列がアクセント記号を含むか判定
    fn contains_accent_marks(s: &str) -> bool {
        s.chars().any(|c| ACCENT_MARKS.contains(&c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let mut tokenizer = Tokenizer::new("こんにちは");
        let tokens = tokenizer.tokenize();
        assert_eq!(tokens, vec![Token::Text("こんにちは".to_string())]);
    }

    #[test]
    fn test_ruby() {
        let mut tokenizer = Tokenizer::new("漢字《かんじ》");
        let tokens = tokenizer.tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::Text("漢字".to_string()),
                Token::Ruby {
                    children: vec![Token::Text("かんじ".to_string())]
                }
            ]
        );
    }

    #[test]
    fn test_prefixed_ruby() {
        let mut tokenizer = Tokenizer::new("｜東京《とうきょう》");
        let tokens = tokenizer.tokenize();
        assert_eq!(
            tokens,
            vec![Token::PrefixedRuby {
                base_children: vec![Token::Text("東京".to_string())],
                ruby_children: vec![Token::Text("とうきょう".to_string())]
            }]
        );
    }

    #[test]
    fn test_command() {
        let mut tokenizer = Tokenizer::new("猫である［＃「である」に傍点］");
        let tokens = tokenizer.tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::Text("猫である".to_string()),
                Token::Command {
                    content: "「である」に傍点".to_string()
                }
            ]
        );
    }

    #[test]
    fn test_gaiji() {
        let mut tokenizer = Tokenizer::new("※［＃「丸印」、U+25CB］");
        let tokens = tokenizer.tokenize();
        assert_eq!(
            tokens,
            vec![Token::Gaiji {
                description: "「丸印」、U+25CB".to_string()
            }]
        );
    }

    #[test]
    fn test_gaiji_mark_alone() {
        let mut tokenizer = Tokenizer::new("※普通の文");
        let tokens = tokenizer.tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::Text("※".to_string()),
                Token::Text("普通の文".to_string())
            ]
        );
    }

    #[test]
    fn test_bracket_without_igeta() {
        // ［の後に＃がないのでコマンドではない
        // ］は単独ではデリミタではないのでテキストの一部になる
        let mut tokenizer = Tokenizer::new("［テスト］");
        let tokens = tokenizer.tokenize();
        assert_eq!(
            tokens,
            vec![
                Token::Text("［".to_string()),
                Token::Text("テスト］".to_string())
            ]
        );
    }

    #[test]
    fn test_nested_command() {
        let mut tokenizer = Tokenizer::new("［＃ここから罫囲み［＃「罫囲み」に傍点］］");
        let tokens = tokenizer.tokenize();
        assert_eq!(
            tokens,
            vec![Token::Command {
                content: "ここから罫囲み［＃「罫囲み」に傍点］".to_string()
            }]
        );
    }
}
