//! レンダリングオプション

/// 参照実装 aozora2html の「バグ由来の出力」に合わせるための互換フラグ。
///
/// 参照実装には、不正な HTML（属性値の中にタグが入るなど）や無意味な CSS
/// （空の幅指定など）を生む挙動がいくつかある。既存の公開 HTML と一致させる
/// ためにはこれらを再現する必要があるが、出力としては望ましくない。
///
/// そこで各挙動をフラグで隔離し、既定ではオン（参照実装に一致）にしつつ、
/// 個別にオフにして規格に沿った出力へ切り替えられるようにする。
/// 将来この互換挙動ごと落とすときは、対応するフラグと分岐を削除すればよい。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quirks {
    /// 外字画像の alt に入れ子の外字記法 `※［＃…］` が含まれるとき、参照実装は
    /// それを alt の中で `<img>` タグに展開する（属性値の中にタグが入る不正な HTML）。
    /// オフにすると入れ子の記法を素のテキストのまま alt に残す。
    pub nested_gaiji_in_alt: bool,
}

impl Default for Quirks {
    fn default() -> Self {
        // 既定は参照実装に一致させる（＝すべて再現する）
        Self {
            nested_gaiji_in_alt: true,
        }
    }
}

impl Quirks {
    /// 参照実装の互換挙動をすべて無効にする（規格に沿った出力にする）
    pub fn none() -> Self {
        Self {
            nested_gaiji_in_alt: false,
        }
    }
}

/// HTML変換オプション
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// 外字画像ディレクトリのパス
    pub gaiji_dir: String,
    /// CSSファイルのパス
    pub css_files: Vec<String>,
    /// JIS X 0213の数値実体参照を使用
    pub use_jisx0213: bool,
    /// Unicodeの数値実体参照を使用
    pub use_unicode: bool,
    /// ドキュメントのタイトル
    pub title: Option<String>,
    /// 参照実装のバグ由来の出力に合わせるための互換フラグ
    pub quirks: Quirks,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            gaiji_dir: "../../../gaiji/".to_string(),
            css_files: vec!["../../aozora.css".to_string()],
            use_jisx0213: false,
            use_unicode: false,
            title: None,
            quirks: Quirks::default(),
        }
    }
}

impl RenderOptions {
    /// 新しいオプションを作成
    pub fn new() -> Self {
        Self::default()
    }

    /// 外字ディレクトリを設定
    pub fn with_gaiji_dir(mut self, dir: impl Into<String>) -> Self {
        self.gaiji_dir = dir.into();
        self
    }

    /// CSSファイルを設定
    pub fn with_css_files(mut self, files: Vec<String>) -> Self {
        self.css_files = files;
        self
    }

    /// JIS X 0213を使用
    pub fn with_jisx0213(mut self, use_it: bool) -> Self {
        self.use_jisx0213 = use_it;
        self
    }

    /// Unicodeを使用
    pub fn with_unicode(mut self, use_it: bool) -> Self {
        self.use_unicode = use_it;
        self
    }

    /// タイトルを設定
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// 互換フラグを設定
    pub fn with_quirks(mut self, quirks: Quirks) -> Self {
        self.quirks = quirks;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options() {
        let opts = RenderOptions::default();
        assert_eq!(opts.gaiji_dir, "../../../gaiji/");
        assert!(!opts.use_jisx0213);
        assert!(!opts.use_unicode);
    }

    #[test]
    fn test_builder_pattern() {
        let opts = RenderOptions::new()
            .with_gaiji_dir("/path/to/gaiji/")
            .with_jisx0213(true)
            .with_title("テスト");

        assert_eq!(opts.gaiji_dir, "/path/to/gaiji/");
        assert!(opts.use_jisx0213);
        assert_eq!(opts.title, Some("テスト".to_string()));
    }
}
