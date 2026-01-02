//! 青空文庫形式のトークン型定義

/// 青空文庫記法のデリミタ（全て全角文字）
pub mod delimiters {
    /// ルビ親文字開始 ｜ (U+FF5C)
    pub const RUBY_PREFIX: char = '｜';

    /// ルビ開始 《 (U+300A)
    pub const RUBY_BEGIN: char = '《';

    /// ルビ終了 》 (U+300B)
    pub const RUBY_END: char = '》';

    /// コマンド開始 ［ (U+FF3B)
    pub const COMMAND_BEGIN: char = '［';

    /// コマンド終了 ］ (U+FF3D)
    pub const COMMAND_END: char = '］';

    /// コマンド識別子 ＃ (U+FF03)
    pub const IGETA: char = '＃';

    /// 外字マーク ※ (U+203B)
    pub const GAIJI_MARK: char = '※';

    /// アクセント開始 〔 (U+3014)
    pub const ACCENT_BEGIN: char = '〔';

    /// アクセント終了 〕 (U+3015)
    pub const ACCENT_END: char = '〕';
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

    /// 外字 ※［＃...］
    Gaiji {
        /// 外字説明（デリミタ除く）
        /// 例: "「二の字点」、1-2-22" や "「丸印」、U+25CB"
        description: String,
    },

    /// アクセント分解 〔...〕
    Accent {
        /// アクセント内のトークン列
        children: Vec<Token>,
    },
}
