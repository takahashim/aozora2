//! 青空文庫形式の字句解析（トークナイザ）

use crate::delimiters::*;
use crate::token::{Span, Token};

/// 1行をトークン列に変換するトークナイザ
pub struct Tokenizer {
    /// 入力をcharとして保持
    chars: Vec<char>,
    /// 現在のchar位置
    pos: usize,
    /// 対応する 〕 の無い 〔 を行末までのアクセントブロックとして扱うか。
    /// 参照実装はトップレベル（行）でのみこれを許し、アクセント内容やルビ等の
    /// 入れ子トークナイズでは未閉じ 〔 をリテラルにする（例:54931 の
    /// 〔訳者注…〔Beethoven…〕 の内側 〔Beethoven）。
    allow_unclosed_accent: bool,
}

impl Tokenizer {
    /// 新しいトークナイザを作成（入れ子用。未閉じ 〔 はリテラル）
    pub fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
            allow_unclosed_accent: false,
        }
    }

    /// トップレベル（行）用トークナイザ。未閉じ 〔 を行末までのアクセントに。
    pub fn new_top_level(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
            allow_unclosed_accent: true,
        }
    }

    /// 入力をトークン列に変換
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        while !self.is_eof() {
            tokens.push(self.next_token());
        }
        tokens
    }

    /// トークンを char 位置範囲（[`Span`]）付きで返す。span は入力先頭からの char
    /// オフセット `[start, end)`。`Tokenizer::new_top_level` の入力（1行）に対する位置。
    pub fn tokenize_spanned(&mut self) -> Vec<(Token, Span)> {
        let mut out = Vec::new();
        while !self.is_eof() {
            let start = self.pos;
            let token = self.next_token();
            out.push((token, Span::new(start, self.pos)));
        }
        out
    }

    /// 現在位置から1トークン読む（tokenize / tokenize_spanned が共有）。
    fn next_token(&mut self) -> Token {
        let ch = self.current_char().unwrap();
        match ch {
            // コマンド ［＃...］ または外字 ※［＃...］の一部
            COMMAND_BEGIN => {
                if self.peek_nth(1) == Some(IGETA) {
                    self.read_command()
                } else {
                    // ［ だけならテキスト
                    self.skip(1);
                    Token::Text(ch.to_string())
                }
            }
            // ルビ 《...》
            RUBY_BEGIN => self.read_ruby(),
            // 明示ルビ ｜...《...》
            RUBY_PREFIX => self.read_prefixed_ruby(),
            // 外字 ※［...］（＃は任意）。※ の次が ［ なら外字扱い（参照 dispatch_gaiji）。
            GAIJI_MARK => {
                if self.peek_nth(1) == Some(COMMAND_BEGIN) {
                    self.read_gaiji()
                } else {
                    self.skip(1);
                    Token::Text(ch.to_string())
                }
            }
            // アクセント 〔...〕
            ACCENT_BEGIN => {
                if let Some(token) = self.try_read_accent() {
                    token
                } else {
                    self.skip(1);
                    Token::Text(ch.to_string())
                }
            }
            // その他はテキスト
            _ => self.read_text(),
        }
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

        // 参照実装 apply_ruby は空ルビ（《》）を《》のテキストとして戻す
        if content.is_empty() {
            return Token::Text("《》".to_string());
        }

        // ルビ内を再帰的にトークナイズ
        let children = Tokenizer::new(&content).tokenize();

        Token::Ruby { children }
    }

    /// 明示ルビトークンを読む ｜...《...》
    ///
    /// 参照実装 RubyBuffer は `｜` のたびに親文字バッファを dump_into して
    /// protected を立て直す。つまり `《》` の直前の**最後の** `｜` からが親文字で、
    /// それより前の `｜…` 区間は（`｜` をリテラルとして残したまま）本文へ出る。
    /// 例: `今日｜民族観念［＃「民族観念」に傍点］と呼ぶ…悲憤｜慷慨《こうがい》`
    /// → `｜` と `民族観念…悲憤` は本文、親文字は `慷慨` だけ。
    /// よって最初の `｜` の後にトップレベル（コマンド ［…］ の外）でもう一つ `｜` が
    /// あれば、最初の `｜` はリテラル扱いにして内容は再トークナイズに任せる。
    fn read_prefixed_ruby(&mut self) -> Token {
        self.skip(1); // ｜
        let base_start = self.pos;

        // base_start から、コマンド ［…］ を飛ばしつつトップレベルの ｜ か 《 を探す。
        let n = self.chars.len();
        let mut scan = base_start;
        while scan < n {
            let c = self.chars[scan];
            if c == COMMAND_BEGIN {
                // ［…］（入れ子可）を丸ごと飛ばす。コマンド内の ｜/《 は区切りでない。
                let mut depth = 0usize;
                while scan < n {
                    match self.chars[scan] {
                        COMMAND_BEGIN => depth += 1,
                        COMMAND_END => {
                            depth -= 1;
                            if depth == 0 {
                                scan += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    scan += 1;
                }
                continue;
            }
            if c == RUBY_PREFIX {
                // 次の ｜ が 《 より先に来た: 最初の ｜ はリテラル。pos は base_start の
                // ままにして、間の内容と次の ｜ は通常のトークナイズに任せる。
                self.pos = base_start;
                return Token::Text(RUBY_PREFIX.to_string());
            }
            if c == RUBY_BEGIN {
                break;
            }
            scan += 1;
        }

        // 《 が見つからなければ ｜ をテキストとして返す
        if scan >= n || self.chars[scan] != RUBY_BEGIN {
            self.pos = base_start;
            return Token::Text(RUBY_PREFIX.to_string());
        }

        let base_content: String = self.chars[base_start..scan].iter().collect();
        self.pos = scan + 1; // 《 の次へ
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

    /// 外字トークンを読む ※［...］（＃は任意）
    fn read_gaiji(&mut self) -> Token {
        self.skip(2); // ※［
        // ＃（IGETA）があれば読み捨てて had_igeta を立てる。
        let had_igeta = self.peek_nth(0) == Some(IGETA);
        if had_igeta {
            self.skip(1);
        }
        let start = self.pos;

        self.skip_until_balanced(COMMAND_BEGIN, COMMAND_END);
        let description = self.slice_from(start);
        self.skip_if(COMMAND_END);

        Token::Gaiji {
            description,
            had_igeta,
        }
    }

    /// アクセントトークンを試行的に読む 〔...〕
    /// アクセント記号がなければNone（テキストとして扱う）
    fn try_read_accent(&mut self) -> Option<Token> {
        let start = self.pos;
        self.skip(1); // 〔
        let content_start = self.pos;

        // 〕 を探す。参照実装は行内に対応する 〕 が無くても、アクセント記号を
        // 含んでいれば 〔 から行末までをアクセントブロックとして処理する（複数行に
        // またがる 〔…改行…〕 では最初の行だけがアクセント化され、次行の 〕 は
        // リテラルになる。例:4363）。見つかった場合のみ 〕 を読み捨てる。
        let found_close = self.skip_until(ACCENT_END);

        // 対応する 〕 が無い場合、トップレベルの行でだけ行末までをアクセントに
        // する。入れ子（アクセント内容・ルビ等）では未閉じ 〔 はリテラルにする。
        if !found_close && !self.allow_unclosed_accent {
            self.pos = start;
            return None;
        }

        let content = self.slice_from(content_start);

        // アクセント記号が無ければアクセントブロックにしない（巻き戻して 〔 は本文）。
        if !Self::contains_accent_marks(&content) {
            self.pos = start;
            return None;
        }

        if found_close {
            self.skip(1); // 〕
        }

        let children = Tokenizer::new(&content).tokenize();
        Some(Token::Accent { children })
    }

    /// 文字列がアクセント表にある組み合わせを含むか判定
    fn contains_accent_marks(s: &str) -> bool {
        crate::accent::contains_accent_sequence(s)
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
    Tokenizer::new_top_level(input).tokenize()
}

/// 文字列を char 位置範囲（[`Span`]）付きトークン列に変換する。
pub fn tokenize_spanned(input: &str) -> Vec<(Token, Span)> {
    Tokenizer::new_top_level(input).tokenize_spanned()
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
    fn test_prefixed_ruby_multiple_pipes() {
        // ｜A｜B《r》: 参照実装は 《》 直前の最後の ｜ からを親文字にし、
        // それより前の ｜A は（｜ をリテラルに残して）本文へ出す。
        let tokens = tokenize("｜東京｜大阪《おおさか》");
        assert_eq!(
            tokens,
            vec![
                Token::Text("｜".to_string()),
                Token::Text("東京".to_string()),
                Token::PrefixedRuby {
                    base_children: vec![Token::Text("大阪".to_string())],
                    ruby_children: vec![Token::Text("おおさか".to_string())],
                }
            ]
        );
    }

    #[test]
    fn test_prefixed_ruby_pipe_inside_command_not_delimiter() {
        // コマンド ［＃…］ の中の ｜ は区切りではない。親文字は 《》 まで伸びる。
        let tokens = tokenize("｜東京［＃「東」に傍点］《とうきょう》");
        assert_eq!(
            tokens,
            vec![Token::PrefixedRuby {
                base_children: vec![
                    Token::Text("東京".to_string()),
                    Token::Command {
                        content: "「東」に傍点".to_string()
                    },
                ],
                ruby_children: vec![Token::Text("とうきょう".to_string())],
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
                description: "「丸印」、U+25CB".to_string(),
                had_igeta: true
            }]
        );
    }

    #[test]
    fn test_unclosed_accent_top_level() {
        // トップレベルの行で 〔 に対応する 〕 が無くアクセント記号を含むなら、
        // 行末までをアクセントブロックにする（参照実装の複数行 〔…改行…〕 の
        // 最初の行の挙動。例:4363「〔Pardonnez a`...」）。
        let tokens = tokenize("〔Pardonnez a` mon");
        assert!(
            matches!(tokens.as_slice(), [Token::Accent { .. }]),
            "未閉じ 〔 がトップレベルでアクセントブロックになっていない: {tokens:?}"
        );
        // 入れ子（アクセント内容の再トークナイズ）では未閉じ 〔 はリテラル。
        // 〔訳者注…〔Beethoven e`…〕 の内側 〔Beethoven は 〔 が本文に残る（54931）。
        let tokens = tokenize("〔訳者注 〔Beethoven e`〕");
        // 外側だけが Accent になり、その中に 〔 テキストが残る。
        assert!(
            matches!(tokens.as_slice(), [Token::Accent { .. }]),
            "外側アクセントブロックが1つにならない: {tokens:?}"
        );
        if let [Token::Accent { children }] = tokens.as_slice() {
            let has_literal_bracket = children
                .iter()
                .any(|t| matches!(t, Token::Text(s) if s.contains('〔')));
            assert!(has_literal_bracket, "内側の未閉じ 〔 がリテラルになっていない: {children:?}");
        }
    }

    #[test]
    fn test_gaiji_without_igeta() {
        // 参照 dispatch_gaiji は「※」の次が「［」なら ＃ 不問で外字扱いする。
        // ＃無しは had_igeta=false（描画時に注記の＃無し・alt名空を再現する）。
        let tokens = tokenize("※［感嘆符二つ、1-8-75］");
        assert_eq!(
            tokens,
            vec![Token::Gaiji {
                description: "感嘆符二つ、1-8-75".to_string(),
                had_igeta: false
            }]
        );
        // 空角括弧 ※［］ も外字（UnEmbedGaiji → ［］ の注記になる）。
        let tokens = tokenize("※［］");
        assert_eq!(
            tokens,
            vec![Token::Gaiji {
                description: String::new(),
                had_igeta: false
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
                    description: "「米印」、U+203B".to_string(),
                    had_igeta: true
                },
                Token::Text("猫である".to_string()),
                Token::Command {
                    content: "「である」に傍点".to_string()
                }
            ]
        );
    }
}
