//! 青空文庫形式の字句解析（トークナイザ）

use crate::delimiters::*;
use crate::token::Token;

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

        while !self.is_eof() {
            let ch = self.current_char().unwrap();

            match ch {
                // コマンド ［＃...］ または外字 ※［＃...］の一部
                COMMAND_BEGIN => {
                    if self.peek_nth(1) == Some(IGETA) {
                        tokens.push(self.read_command());
                    } else {
                        // ［ だけならテキスト
                        tokens.push(Token::Text(ch.to_string()));
                        self.skip(1);
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
                    if self.peek_nth(1) == Some(COMMAND_BEGIN) && self.peek_nth(2) == Some(IGETA) {
                        tokens.push(self.read_gaiji());
                    } else {
                        // ※ だけならテキスト
                        tokens.push(Token::Text(ch.to_string()));
                        self.skip(1);
                    }
                }

                // アクセント 〔...〕
                ACCENT_BEGIN => {
                    if let Some(token) = self.try_read_accent() {
                        tokens.push(token);
                    } else {
                        // アクセント記号がなければテキスト
                        tokens.push(Token::Text(ch.to_string()));
                        self.skip(1);
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

    // --- トークン読み取り ---

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
        self.skip(2); // ［＃
        let start = self.pos;

        self.skip_until_balanced(COMMAND_BEGIN, COMMAND_END);
        let content = self.slice_from(start);
        self.skip_if(COMMAND_END);

        Token::Command { content }
    }

    /// ルビトークンを読む 《...》
    fn read_ruby(&mut self) -> Token {
        self.skip(1); // 《
        let start = self.pos;

        self.skip_until(RUBY_END);
        let content = self.slice_from(start);
        self.skip_if(RUBY_END);

        // ルビ内を再帰的にトークナイズ
        let children = Tokenizer::new(&content).tokenize();

        Token::Ruby { children }
    }

    /// 明示ルビトークンを読む ｜...《...》
    fn read_prefixed_ruby(&mut self) -> Token {
        self.skip(1); // ｜
        let base_start = self.pos;

        // 《 が見つからなければ ｜ をテキストとして返す
        if !self.skip_until(RUBY_BEGIN) {
            self.pos = base_start;
            return Token::Text(RUBY_PREFIX.to_string());
        }

        let base_content = self.slice_from(base_start);
        self.skip(1); // 《
        let ruby_start = self.pos;

        self.skip_until(RUBY_END);
        let ruby_content = self.slice_from(ruby_start);
        self.skip_if(RUBY_END);

        // 親文字とルビを再帰的にトークナイズ
        let base_children = Tokenizer::new(&base_content).tokenize();
        let ruby_children = Tokenizer::new(&ruby_content).tokenize();

        Token::PrefixedRuby {
            base_children,
            ruby_children,
        }
    }

    /// 外字トークンを読む ※［＃...］
    fn read_gaiji(&mut self) -> Token {
        self.skip(3); // ※［＃
        let start = self.pos;

        self.skip_until_balanced(COMMAND_BEGIN, COMMAND_END);
        let description = self.slice_from(start);
        self.skip_if(COMMAND_END);

        Token::Gaiji { description }
    }

    /// アクセントトークンを試行的に読む 〔...〕
    /// アクセント記号がなければNone（テキストとして扱う）
    fn try_read_accent(&mut self) -> Option<Token> {
        let start = self.pos;
        self.skip(1); // 〔
        let content_start = self.pos;

        // 〕 が見つからない、またはアクセント記号がなければ巻き戻し
        if !self.skip_until(ACCENT_END) {
            self.pos = start;
            return None;
        }

        let content = self.slice_from(content_start);

        if !Self::contains_accent_marks(&content) {
            self.pos = start;
            return None;
        }

        self.skip(1); // 〕

        let children = Tokenizer::new(&content).tokenize();
        Some(Token::Accent { children })
    }

    /// 文字列がアクセント記号を含むか判定
    fn contains_accent_marks(s: &str) -> bool {
        s.chars().any(|c| ACCENT_MARKS.contains(&c))
    }

    // --- カーソル操作ヘルパー ---

    /// 入力の終端に達したか
    fn is_eof(&self) -> bool {
        self.pos >= self.chars.len()
    }

    /// 現在位置から n 文字先を覗く
    fn peek_nth(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    /// 現在の文字を取得
    fn current_char(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// n 文字スキップ
    fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    /// 特定の文字までスキップ（見つかったらtrue）
    fn skip_until(&mut self, target: char) -> bool {
        while self.pos < self.chars.len() {
            if self.chars[self.pos] == target {
                return true;
            }
            self.pos += 1;
        }
        false
    }

    /// ネストを考慮して閉じ括弧までスキップ（閉じ括弧の手前で停止）
    fn skip_until_balanced(&mut self, open: char, close: char) {
        let mut depth = 1;
        while self.pos < self.chars.len() && depth > 0 {
            let ch = self.chars[self.pos];
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth -= 1;
            }
            if depth > 0 {
                self.pos += 1;
            }
        }
    }

    /// 現在の文字が target なら1文字スキップ
    fn skip_if(&mut self, target: char) {
        if self.current_char() == Some(target) {
            self.pos += 1;
        }
    }

    /// start から現在位置までを文字列として取得
    fn slice_from(&self, start: usize) -> String {
        self.chars[start..self.pos].iter().collect()
    }
}

/// 文字列をトークン列に変換するユーティリティ関数
pub fn tokenize(input: &str) -> Vec<Token> {
    Tokenizer::new(input).tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let tokens = tokenize("こんにちは");
        assert_eq!(tokens, vec![Token::Text("こんにちは".to_string())]);
    }

    #[test]
    fn test_ruby() {
        let tokens = tokenize("漢字《かんじ》");
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
        let tokens = tokenize("｜東京《とうきょう》");
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
        let tokens = tokenize("猫である［＃「である」に傍点］");
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
        let tokens = tokenize("※［＃「丸印」、U+25CB］");
        assert_eq!(
            tokens,
            vec![Token::Gaiji {
                description: "「丸印」、U+25CB".to_string()
            }]
        );
    }

    #[test]
    fn test_gaiji_mark_alone() {
        let tokens = tokenize("※普通の文");
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
        let tokens = tokenize("［テスト］");
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
        let tokens = tokenize("［＃ここから罫囲み［＃「罫囲み」に傍点］］");
        assert_eq!(
            tokens,
            vec![Token::Command {
                content: "ここから罫囲み［＃「罫囲み」に傍点］".to_string()
            }]
        );
    }

    #[test]
    fn test_accent() {
        let tokens = tokenize("〔E'difice〕");
        assert_eq!(
            tokens,
            vec![Token::Accent {
                children: vec![Token::Text("E'difice".to_string())]
            }]
        );
    }

    #[test]
    fn test_accent_no_mark() {
        let tokens = tokenize("〔参考〕");
        assert_eq!(
            tokens,
            vec![
                Token::Text("〔".to_string()),
                Token::Text("参考〕".to_string())
            ]
        );
    }

    #[test]
    fn test_prefixed_ruby_without_ruby() {
        let tokens = tokenize("｜だけ");
        assert_eq!(
            tokens,
            vec![
                Token::Text("｜".to_string()),
                Token::Text("だけ".to_string())
            ]
        );
    }

    #[test]
    fn test_empty_input() {
        let tokens = tokenize("");
        assert_eq!(tokens, vec![]);
    }

    #[test]
    fn test_multiple_tokens() {
        let tokens =
            tokenize("吾輩《わがはい》は※［＃「米印」、U+203B］猫である［＃「である」に傍点］");
        assert_eq!(
            tokens,
            vec![
                Token::Text("吾輩".to_string()),
                Token::Ruby {
                    children: vec![Token::Text("わがはい".to_string())]
                },
                Token::Text("は".to_string()),
                Token::Gaiji {
                    description: "「米印」、U+203B".to_string()
                },
                Token::Text("猫である".to_string()),
                Token::Command {
                    content: "「である」に傍点".to_string()
                }
            ]
        );
    }
}
