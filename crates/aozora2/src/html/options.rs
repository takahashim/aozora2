//! レンダリングオプション

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
    /// 完全なHTMLドキュメントを生成（html, head, bodyタグを含む）
    pub full_document: bool,
    /// ドキュメントのタイトル
    pub title: Option<String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            gaiji_dir: "../../../gaiji/".to_string(),
            css_files: vec!["../../aozora.css".to_string()],
            use_jisx0213: false,
            use_unicode: false,
            full_document: false,
            title: None,
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

    /// 完全なHTMLドキュメントを生成
    pub fn with_full_document(mut self, full: bool) -> Self {
        self.full_document = full;
        self
    }

    /// タイトルを設定
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
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
        assert!(!opts.full_document);
    }

    #[test]
    fn test_builder_pattern() {
        let opts = RenderOptions::new()
            .with_gaiji_dir("/path/to/gaiji/")
            .with_jisx0213(true)
            .with_full_document(true)
            .with_title("テスト");

        assert_eq!(opts.gaiji_dir, "/path/to/gaiji/");
        assert!(opts.use_jisx0213);
        assert!(opts.full_document);
        assert_eq!(opts.title, Some("テスト".to_string()));
    }
}
