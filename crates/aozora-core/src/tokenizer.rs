//! 青空文庫形式の字句解析（トークナイザ）

use crate::delimiters::*;
use crate::token::{Span, Token, TokenKind};

/// 1行をトークン列に変換するトークナイザ
pub struct Tokenizer {
    /// 入力をcharとして保持
    chars: Vec<char>,
    /// 現在のchar位置
    pos: usize,
    /// この入力断片の、元の行における開始charオフセット。
    base: usize,
    /// 対応する 〕 の無い 〔 を行末までのアクセントブロックとして扱うか。
    /// 参照実装はトップレベル（行）でのみこれを許し、アクセント内容やルビ等の
    /// 入れ子トークナイズでは未閉じ 〔 をリテラルにする（例:54931 の
    /// 〔訳者注…〔Beethoven…〕 の内側 〔Beethoven）。
    allow_unclosed_accent: bool,
    /// 未閉じのまま行末まで延長したアクセント（＝複数行アクセントの1行目）の絶対 span。
    /// 変換には無関係の**検証用の副産物**。トップレベルでのみ溜まる（入れ子は未閉じ 〔 を
    /// リテラルにするので発生しない）。診断 `unclosed-accent` や厳格モードが使う。
    unclosed_accent_spans: Vec<Span>,
}

impl Tokenizer {
    /// トップレベル（行）用トークナイザ。未閉じ 〔 を行末までのアクセントに。
    pub fn new_top_level(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
            base: 0,
            allow_unclosed_accent: true,
            unclosed_accent_spans: Vec::new(),
        }
    }

    /// 入力を [`Token`] 列に変換する。各トークンには入力行先頭からのchar位置
    /// 範囲（[`Span`]、`[start, end)`）を付ける。入れ子のトークンも絶対位置を持つ。
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut out = Vec::new();
        while !self.is_eof() {
            let start = self.pos;
            let token = self.next_token();
            out.push(Token::new(
                token,
                Span::new(self.base + start, self.base + self.pos),
            ));
        }
        // ｜ は字句段階では単なるマーカー（RubyPrefix）として出るだけ。ここで
        // 直後のルビと畳んで明示ルビ（PrefixedRuby）を確定する。tokenize_children
        // 経由でも呼ばれるので、入れ子の ｜ も同じ規則で畳まれる。
        fold_prefixed_ruby(out)
    }

    /// 入れ子内容（ルビ・親文字・アクセント内）を絶対char位置付きでトークナイズする。
    fn tokenize_children(input: &str, base: usize) -> Vec<Token> {
        let mut tokenizer = Self {
            chars: input.chars().collect(),
            pos: 0,
            base,
            allow_unclosed_accent: false,
            unclosed_accent_spans: Vec::new(),
        };
        tokenizer.tokenize()
    }

    /// 現在位置から1トークン読む（`tokenize` が使う）。
    fn next_token(&mut self) -> TokenKind {
        let ch = self.current_char().unwrap();
        match ch {
            // コマンド ［＃...］ または外字 ※［＃...］の一部
            COMMAND_BEGIN => {
                if self.peek_nth(1) == Some(IGETA) {
                    self.read_command()
                } else {
                    // ［ だけならテキスト
                    self.skip(1);
                    TokenKind::Text(ch.to_string())
                }
            }
            // ルビ 《...》
            RUBY_BEGIN => self.read_ruby(),
            // 明示ルビ前置 ｜ … 字句段階ではマーカーを出すだけ（親文字確定は後段 fold）。
            RUBY_PREFIX => {
                self.skip(1);
                TokenKind::RubyPrefix
            }
            // 外字 ※［...］（＃は任意）。※ の次が ［ なら外字扱い（参照 dispatch_gaiji）。
            GAIJI_MARK => {
                if self.peek_nth(1) == Some(COMMAND_BEGIN) {
                    self.read_gaiji()
                } else {
                    self.skip(1);
                    TokenKind::Text(ch.to_string())
                }
            }
            // アクセント 〔...〕
            ACCENT_BEGIN => {
                if let Some(token) = self.try_read_accent() {
                    token
                } else {
                    self.skip(1);
                    TokenKind::Text(ch.to_string())
                }
            }
            // その他はテキスト
            _ => self.read_text(),
        }
    }

    // --- トークン読み取り ---

    /// テキストトークンを読む（デリミタまで）
    fn read_text(&mut self) -> TokenKind {
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
        TokenKind::Text(text)
    }

    /// コマンドトークンを読む ［＃...］
    /// ネストに対応（括弧の深さを追跡）
    fn read_command(&mut self) -> TokenKind {
        self.skip(2); // ［＃
        let start = self.pos;

        self.skip_until_balanced(COMMAND_BEGIN, COMMAND_END);
        let content = self.slice_from(start);
        self.skip_if(COMMAND_END);

        TokenKind::Command { content }
    }

    /// ルビトークンを読む 《...》
    fn read_ruby(&mut self) -> TokenKind {
        self.skip(1); // 《
        let start = self.pos;

        self.skip_until(RUBY_END);
        let content = self.slice_from(start);
        self.skip_if(RUBY_END);

        // 参照実装 apply_ruby は空ルビ（《》）を《》のテキストとして戻す
        if content.is_empty() {
            return TokenKind::Text("《》".to_string());
        }

        // ルビ内を再帰的にトークナイズ
        let children = Self::tokenize_children(&content, self.base + start);

        TokenKind::Ruby { children }
    }

    /// 外字トークンを読む ※［...］（＃は任意）
    fn read_gaiji(&mut self) -> TokenKind {
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

        TokenKind::Gaiji {
            description,
            had_igeta,
        }
    }

    /// アクセントトークンを試行的に読む 〔...〕
    /// アクセント記号がなければNone（テキストとして扱う）
    fn try_read_accent(&mut self) -> Option<TokenKind> {
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
        } else {
            // 対応する 〕 が同一行に無いまま行末まで延長した（＝複数行アクセントの1行目、
            // あるいは閉じ忘れ）。変換は参照実装どおり受理するが、検証用に span を控える。
            self.unclosed_accent_spans
                .push(Span::new(self.base + start, self.base + self.pos));
        }

        let children = Self::tokenize_children(&content, self.base + content_start);
        Some(TokenKind::Accent { children })
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

/// 一時マーカー [`TokenKind::RubyPrefix`]（＝`｜`）を、直後のルビと畳んで
/// [`TokenKind::PrefixedRuby`] にする。字句解析の直後に一度だけ走る。
///
/// アルゴリズムは「`《》` から左へ辿って最も近い `｜` が親文字開始」という後方規則
/// （spec-tokenizer.md 参照）を、逆向きに実装したもの:
/// - ルビ（暗黙 `Ruby`）を見つけたら **`out` を末尾から遡り**、別のルビ
///   （`Ruby`/`PrefixedRuby`）を越えない範囲で最も近い `RubyPrefix` を探す。
/// - 見つかれば、そのマーカー〜ルビ直前を**親文字**として `PrefixedRuby` に畳む
///   （親文字は空でもよい。例:`一番向｜《むか》` → 親文字空）。
/// - 見つからなければ暗黙ルビのまま（親文字は後段の文字種抽出に委ねる）。
/// - どのルビにも畳まれず余った `RubyPrefix` は本文の `｜`（`Text`）へ戻す。
///   これで「多重 `｜` は最後が親文字・前はリテラル」が自動的に従う。
/// - コマンド `［＃…］` は1トークンなので、その中の `｜` はマーカーにならず透過。
fn fold_prefixed_ruby(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    for token in tokens {
        let Token { kind, span } = token;
        if let TokenKind::Ruby { children } = kind {
            // 末尾から遡り、別のルビを越えない範囲で最も近い ｜ マーカーを探す。
            let mut marker = None;
            for idx in (0..out.len()).rev() {
                match &out[idx].kind {
                    TokenKind::RubyPrefix => {
                        marker = Some(idx);
                        break;
                    }
                    TokenKind::Ruby { .. } | TokenKind::PrefixedRuby { .. } => break,
                    _ => {}
                }
            }
            match marker {
                Some(m) => {
                    let marker_span = out[m].span;
                    let mut base_children = out.split_off(m);
                    base_children.remove(0); // ｜ マーカーを捨て、残りを親文字にする
                    out.push(Token::new(
                        TokenKind::PrefixedRuby {
                            base_children,
                            ruby_children: children,
                        },
                        marker_span.union(span),
                    ));
                }
                None => out.push(Token::new(TokenKind::Ruby { children }, span)),
            }
        } else {
            out.push(Token::new(kind, span));
        }
    }
    // 余った ｜ マーカーはリテラルの本文 ｜ に戻す。
    for token in out.iter_mut() {
        if matches!(token.kind, TokenKind::RubyPrefix) {
            token.kind = TokenKind::Text(RUBY_PREFIX.to_string());
        }
    }
    out
}

/// 文字列を [`Token`] 列に変換するユーティリティ関数。各spanは入力先頭からの
/// char オフセット `[start, end)`。
///
/// **1 行**（改行を含まない文字列）を渡す前提。複数行を渡すと `\n` はただのテキストになり、
/// 未閉じ `〔` の「行末まで」が「入力末まで」に変わる。
pub fn tokenize(input: &str) -> Vec<Token> {
    Tokenizer::new_top_level(input).tokenize()
}

/// [`tokenize`] と同じトークン列に加え、**未閉じのまま行末まで延長したアクセント**
/// （＝複数行アクセントの1行目・閉じ忘れ）の絶対 span を返す。
///
/// トークン列は [`tokenize`] と完全に同一（byte 一致に無影響）で、span を副産物として
/// 返すだけ。検証・診断（`unclosed-accent`）・将来の厳格モードが使う。1 行を渡す前提。
pub fn tokenize_collecting_unclosed_accents(input: &str) -> (Vec<Token>, Vec<Span>) {
    let mut tokenizer = Tokenizer::new_top_level(input);
    let tokens = tokenizer.tokenize();
    (tokens, tokenizer.unclosed_accent_spans)
}

#[cfg(test)]
mod tests {
    use super::tokenize;
    use crate::token::{Token as ActualToken, TokenKind};

    /// span を除いてトークン構造を比較するためのテスト用表現。
    #[derive(Debug, PartialEq)]
    enum Token {
        Text(String),
        Ruby {
            children: Vec<Token>,
        },
        PrefixedRuby {
            base_children: Vec<Token>,
            ruby_children: Vec<Token>,
        },
        Command {
            content: String,
        },
        Gaiji {
            description: String,
            had_igeta: bool,
        },
        Accent {
            children: Vec<Token>,
        },
    }

    impl From<ActualToken> for Token {
        fn from(token: ActualToken) -> Self {
            match token.kind {
                TokenKind::Text(text) => Self::Text(text),
                TokenKind::Ruby { children } => Self::Ruby {
                    children: children.into_iter().map(Self::from).collect(),
                },
                TokenKind::PrefixedRuby {
                    base_children,
                    ruby_children,
                } => Self::PrefixedRuby {
                    base_children: base_children.into_iter().map(Self::from).collect(),
                    ruby_children: ruby_children.into_iter().map(Self::from).collect(),
                },
                TokenKind::Command { content } => Self::Command { content },
                TokenKind::Gaiji {
                    description,
                    had_igeta,
                } => Self::Gaiji {
                    description,
                    had_igeta,
                },
                TokenKind::Accent { children } => Self::Accent {
                    children: children.into_iter().map(Self::from).collect(),
                },
                TokenKind::RubyPrefix => {
                    unreachable!("RubyPrefix markers are folded away inside tokenize()")
                }
            }
        }
    }

    /// span を落として Token 列だけを比較するテスト用ヘルパー。
    fn plain(input: &str) -> Vec<Token> {
        tokenize(input).into_iter().map(Token::from).collect()
    }

    fn assert_spans_are_well_formed(
        tokens: &[ActualToken],
        source_len: usize,
        parent: Option<crate::token::Span>,
    ) {
        let mut previous_end = 0;
        for token in tokens {
            assert!(token.span.start <= token.span.end);
            assert!(token.span.end <= source_len);
            assert!(token.span.start >= previous_end, "siblings must be ordered");
            if let Some(parent) = parent {
                assert!(parent.start <= token.span.start && token.span.end <= parent.end);
            }
            previous_end = token.span.end;

            match &token.kind {
                TokenKind::Ruby { children } | TokenKind::Accent { children } => {
                    assert_spans_are_well_formed(children, source_len, Some(token.span));
                }
                TokenKind::PrefixedRuby {
                    base_children,
                    ruby_children,
                } => {
                    assert_spans_are_well_formed(base_children, source_len, Some(token.span));
                    assert_spans_are_well_formed(ruby_children, source_len, Some(token.span));
                }
                TokenKind::Text(_)
                | TokenKind::Command { .. }
                | TokenKind::Gaiji { .. }
                | TokenKind::RubyPrefix => {}
            }
        }
    }

    #[test]
    fn test_plain_text() {
        let tokens = plain("こんにちは");
        assert_eq!(tokens, vec![Token::Text("こんにちは".to_string())]);
    }

    #[test]
    fn test_ruby() {
        let tokens = plain("漢字《かんじ》");
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
        let tokens = plain("｜東京《とうきょう》");
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
        let tokens = plain("｜東京｜大阪《おおさか》");
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
        let tokens = plain("｜東京［＃「東」に傍点］《とうきょう》");
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
        let tokens = plain("猫である［＃「である」に傍点］");
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
        let tokens = plain("※［＃「丸印」、U+25CB］");
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
        let tokens = plain("〔Pardonnez a` mon");
        assert!(
            matches!(tokens.as_slice(), [Token::Accent { .. }]),
            "未閉じ 〔 がトップレベルでアクセントブロックになっていない: {tokens:?}"
        );
        // 入れ子（アクセント内容の再トークナイズ）では未閉じ 〔 はリテラル。
        // 〔訳者注…〔Beethoven e`…〕 の内側 〔Beethoven は 〔 が本文に残る（54931）。
        let tokens = plain("〔訳者注 〔Beethoven e`〕");
        // 外側だけが Accent になり、その中に 〔 テキストが残る。
        assert!(
            matches!(tokens.as_slice(), [Token::Accent { .. }]),
            "外側アクセントブロックが1つにならない: {tokens:?}"
        );
        if let [Token::Accent { children }] = tokens.as_slice() {
            let has_literal_bracket = children
                .iter()
                .any(|t| matches!(t, Token::Text(s) if s.contains('〔')));
            assert!(
                has_literal_bracket,
                "内側の未閉じ 〔 がリテラルになっていない: {children:?}"
            );
        }
    }

    #[test]
    fn test_unclosed_constructs_swallow_to_end_of_line() {
        // spec-tokenizer.md「未閉じ構文の扱い」: 巻き戻して literal にするのは入れ子の
        // 〔 だけ。《 ［＃ ※［ はいずれも閉じ記号が無ければ行末まで飲み込む（拒否しない）。
        assert_eq!(
            plain("あ《かな"),
            vec![
                Token::Text("あ".to_string()),
                Token::Ruby {
                    children: vec![Token::Text("かな".to_string())]
                }
            ]
        );
        assert_eq!(
            plain("あ［＃注記"),
            vec![
                Token::Text("あ".to_string()),
                Token::Command {
                    content: "注記".to_string()
                }
            ]
        );
        assert_eq!(
            plain("あ※［＃外字"),
            vec![
                Token::Text("あ".to_string()),
                Token::Gaiji {
                    description: "外字".to_string(),
                    had_igeta: true
                }
            ]
        );
    }

    #[test]
    fn test_closing_delimiters_alone_are_plain_text() {
        // 閉じ記号は read_text の区切りではないので、単独で現れたらただの本文。
        assert_eq!(plain("あ》い"), vec![Token::Text("あ》い".to_string())]);
        assert_eq!(plain("あ〕い"), vec![Token::Text("あ〕い".to_string())]);
    }

    #[test]
    fn test_gaiji_without_igeta() {
        // 参照 dispatch_gaiji は「※」の次が「［」なら ＃ 不問で外字扱いする。
        // ＃無しは had_igeta=false（描画時に注記の＃無し・alt名空を再現する）。
        let tokens = plain("※［感嘆符二つ、1-8-75］");
        assert_eq!(
            tokens,
            vec![Token::Gaiji {
                description: "感嘆符二つ、1-8-75".to_string(),
                had_igeta: false
            }]
        );
        // 空角括弧 ※［］ も外字（UnEmbedGaiji → ［］ の注記になる）。
        let tokens = plain("※［］");
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
        let tokens = plain("※普通の文");
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
        let tokens = plain("［テスト］");
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
        let tokens = plain("［＃ここから罫囲み［＃「罫囲み」に傍点］］");
        assert_eq!(
            tokens,
            vec![Token::Command {
                content: "ここから罫囲み［＃「罫囲み」に傍点］".to_string()
            }]
        );
    }

    #[test]
    fn test_accent() {
        let tokens = plain("〔E'difice〕");
        assert_eq!(
            tokens,
            vec![Token::Accent {
                children: vec![Token::Text("E'difice".to_string())]
            }]
        );
    }

    #[test]
    fn test_accent_no_mark() {
        let tokens = plain("〔参考〕");
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
        let tokens = plain("｜だけ");
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
        let tokens = plain("");
        assert_eq!(tokens, vec![]);
    }

    #[test]
    fn test_multiple_tokens() {
        let tokens =
            plain("吾輩《わがはい》は※［＃「米印」、U+203B］猫である［＃「である」に傍点］");
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

    #[test]
    fn token_spans_are_absolute_for_nested_constructs() {
        let tokens = tokenize("A《B※［＃x］》C");
        assert_spans_are_well_formed(&tokens, "A《B※［＃x］》C".chars().count(), None);
        assert_eq!(tokens[0].span, crate::token::Span::new(0, 1));
        assert_eq!(tokens[1].span, crate::token::Span::new(1, 9));
        assert_eq!(tokens[2].span, crate::token::Span::new(9, 10));
        let TokenKind::Ruby { children } = &tokens[1].kind else {
            panic!("expected ruby token");
        };
        assert_eq!(children[0].span, crate::token::Span::new(2, 3));
        assert_eq!(children[1].span, crate::token::Span::new(3, 8));

        let tokens = tokenize("｜東京《とう》");
        assert_spans_are_well_formed(&tokens, "｜東京《とう》".chars().count(), None);
        assert_eq!(tokens[0].span, crate::token::Span::new(0, 7));
        let TokenKind::PrefixedRuby {
            base_children,
            ruby_children,
        } = &tokens[0].kind
        else {
            panic!("expected prefixed ruby token");
        };
        assert_eq!(base_children[0].span, crate::token::Span::new(1, 3));
        assert_eq!(ruby_children[0].span, crate::token::Span::new(4, 6));

        let tokens = tokenize("〔A《B》e'〕");
        assert_spans_are_well_formed(&tokens, "〔A《B》e'〕".chars().count(), None);
        assert_eq!(tokens[0].span, crate::token::Span::new(0, 8));
        let TokenKind::Accent { children } = &tokens[0].kind else {
            panic!("expected accent token");
        };
        assert_eq!(children[0].span, crate::token::Span::new(1, 2));
        assert_eq!(children[1].span, crate::token::Span::new(2, 5));
        assert_eq!(children[2].span, crate::token::Span::new(5, 7));
        let TokenKind::Ruby { children: ruby } = &children[1].kind else {
            panic!("expected nested ruby token");
        };
        assert_eq!(ruby[0].span, crate::token::Span::new(3, 4));
    }

    #[test]
    fn token_spans_cover_empty_ruby_and_unclosed_accent() {
        let tokens = tokenize("《》");
        assert_spans_are_well_formed(&tokens, "《》".chars().count(), None);
        assert!(matches!(&tokens[0].kind, TokenKind::Text(text) if text == "《》"));
        assert_eq!(tokens[0].span, crate::token::Span::new(0, 2));

        let tokens = tokenize("〔e'");
        assert_spans_are_well_formed(&tokens, "〔e'".chars().count(), None);
        assert_eq!(tokens[0].span, crate::token::Span::new(0, 3));
        let TokenKind::Accent { children } = &tokens[0].kind else {
            panic!("expected unclosed accent token");
        };
        assert_eq!(children[0].span, crate::token::Span::new(1, 3));
    }
}
