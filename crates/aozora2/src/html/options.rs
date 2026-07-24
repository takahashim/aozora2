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
    /// 図中の画像に幅・高さの指定がないとき、参照実装は `width="" height=""` と
    /// 空の属性を出す（Ruby の nil 展開の副産物で、意味のない属性）。
    /// オフにすると幅・高さが指定されたときだけ属性を出す。
    pub empty_image_dimensions: bool,
    /// 参照実装のアクセント表 accent_table.yml では、サーカムフレックス付き A の
    /// 説明が「サーカムフレックスアクセント付き」と字母 A を落としている
    /// （E^〜U^ は「…付きE」等と字母付きなので、A^ だけの誤植）。
    /// オフにすると「サーカムフレックスアクセント付きA」に訂正する。
    pub accent_name_typos: bool,
    /// 参照実装はヘッダのタイトル・著者等（h1.title / h2.author など、および
    /// DC.Title / DC.Creator メタ）を素の文字列のまま出力し、`&<>"` を
    /// エスケープしない。タイトルに `"` を含む場合、meta 属性値が壊れるなど
    /// 規格上不正な HTML になりうる。オフにするとこれらをエスケープする。
    pub raw_header_metadata: bool,
    /// 字下げ／ぶら下げの幅が空（参照実装の `(\d*)字下げ` が隣接数字を取れない
    /// `３　字下げ` や、コンマなし `折り返してN字下げ` など）のとき、参照実装は
    /// `class="jisage_"` や `margin-left: em`（数値の無い不正な CSS 長さ）を出す。
    /// `margin-left: em` は無効値としてブラウザに無視され、`.jisage_`（空サフィックス）
    /// も対応する CSS 規則が無いので、意味の無い出力になる。オフにすると空幅を
    /// 出さず妥当な CSS にする（jisage は `class="jisage"`、ぶら下げは margin-left 0）。
    pub empty_indent_css: bool,
}

impl Default for Quirks {
    fn default() -> Self {
        // 既定は参照実装に一致させる（＝すべて再現する）
        Self {
            nested_gaiji_in_alt: true,
            empty_image_dimensions: true,
            accent_name_typos: true,
            raw_header_metadata: true,
            empty_indent_css: true,
        }
    }
}

impl Quirks {
    /// 参照実装の互換挙動をすべて無効にする（規格に沿った出力にする）
    pub fn none() -> Self {
        Self {
            nested_gaiji_in_alt: false,
            empty_image_dimensions: false,
            accent_name_typos: false,
            raw_header_metadata: false,
            empty_indent_css: false,
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
