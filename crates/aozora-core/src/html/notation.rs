//! フッタ「表記について」の材料。
//!
//! 参照実装では TagParser/BlockManager が使用フラグ（`:chuki` / `:newjis` /
//! `:accent` 等）と外字一覧を持ち、本文描画の副作用で立てる。ここはその器で、
//! HTML の組み立ては持たない（出力は
//! [`super::document_renderer::DocumentRenderer::render_notation_notes`]）。

/// くの字点（繰り返し記号）の構成文字。
const KUNOJI_KU: char = '／';
const KUNOJI_NOJI: char = '＼';
const KUNOJI_DAKUTEN: char = '″';

/// 未変換外字情報（フッタ「表記について」の外字一覧用）。
#[derive(Debug, Clone)]
pub struct UnconvertedGaiji {
    /// 外字名（説明の最後の「、」より前の部分）
    pub gaiji_name: String,
    /// ページ-行数（説明の最後の「、」より後の部分）。
    /// 同じ外字が複数回現れた場合は出現箇所を順に並べる。
    pub page_lines: Vec<String>,
}

/// 本文描画の副作用として溜まる「表記について」の状態。
#[derive(Debug, Clone, Default)]
pub struct NotationState {
    has_notes: bool,
    has_gaiji_images: bool,
    has_accent: bool,
    has_jisx0213: bool,
    has_kunoji: bool,
    has_dakuten_kunoji: bool,
    unconverted_gaiji: Vec<UnconvertedGaiji>,
}

impl NotationState {
    /// 注記を使用した
    pub fn mark_notes(&mut self) {
        self.has_notes = true;
    }

    /// 外字画像を使用した
    pub fn mark_gaiji_image(&mut self) {
        self.has_gaiji_images = true;
    }

    /// アクセント記号を使用した
    pub fn mark_accent(&mut self) {
        self.has_accent = true;
    }

    /// JIS X 0213 文字を使用した
    pub fn mark_jisx0213(&mut self) {
        self.has_jisx0213 = true;
    }

    /// 注記を使用したか
    pub fn has_notes(&self) -> bool {
        self.has_notes
    }

    /// 外字画像を使用したか。
    /// 参照実装の「表記について」に対応する項目が無いため現状どこも読まない
    /// （`mark_gaiji_image` の呼び出し箇所は参照 `:newjis` に対応）。
    #[allow(dead_code)]
    pub fn has_gaiji_images(&self) -> bool {
        self.has_gaiji_images
    }

    /// アクセント記号を使用したか
    pub fn has_accent(&self) -> bool {
        self.has_accent
    }

    /// JIS X 0213 文字を使用したか
    pub fn has_jisx0213(&self) -> bool {
        self.has_jisx0213
    }

    /// くの字点を使用したか
    pub fn has_kunoji(&self) -> bool {
        self.has_kunoji
    }

    /// 濁点付きくの字点を使用したか
    pub fn has_dakuten_kunoji(&self) -> bool {
        self.has_dakuten_kunoji
    }

    /// 未変換外字の一覧
    pub fn unconverted_gaiji(&self) -> &[UnconvertedGaiji] {
        &self.unconverted_gaiji
    }

    /// くの字点を数える（参照 scan_kunoji）。
    /// 注記の中にも書かれうるので、パース後ではなく生のソース行を渡すこと。
    pub fn scan_kunoji(&mut self, text: &str) {
        if self.has_kunoji && self.has_dakuten_kunoji {
            return;
        }
        let chars: Vec<char> = text.chars().collect();
        for (i, c) in chars.iter().enumerate() {
            if *c != KUNOJI_KU {
                continue;
            }
            match chars.get(i + 1) {
                Some(&KUNOJI_NOJI) => self.has_kunoji = true,
                Some(&KUNOJI_DAKUTEN) if chars.get(i + 2) == Some(&KUNOJI_NOJI) => {
                    self.has_dakuten_kunoji = true
                }
                _ => {}
            }
        }
    }

    /// 画像化できない外字を一覧に加える。同じ外字名が既にあれば出現箇所を足す。
    /// ＃無しの外字 `※［...］` は参照 PAT_GAIJI が ＃ を必須とするため名前・
    /// 出現箇所とも空になる。
    pub fn add_unconverted_gaiji(&mut self, description: &str, had_igeta: bool) {
        let (gaiji_name, page_line) = if !had_igeta {
            (String::new(), String::new())
        } else {
            match description.rfind('、') {
                Some(pos) => (
                    description[..pos].to_string(),
                    description[pos + '、'.len_utf8()..].to_string(),
                ),
                None => (String::new(), String::new()),
            }
        };
        if let Some(existing) = self
            .unconverted_gaiji
            .iter_mut()
            .find(|g| g.gaiji_name == gaiji_name)
        {
            existing.page_lines.push(page_line);
            return;
        }
        self.unconverted_gaiji.push(UnconvertedGaiji {
            gaiji_name,
            page_lines: vec![page_line],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// くの字点・濁点付きくの字点をそれぞれ独立に検出する。
    #[test]
    fn test_scan_kunoji_detects_each_form() {
        let mut n = NotationState::default();
        n.scan_kunoji("わざ／＼と");
        assert!(n.has_kunoji());
        assert!(!n.has_dakuten_kunoji());

        n.scan_kunoji("しみ／″＼と");
        assert!(n.has_dakuten_kunoji());
    }

    /// ／ や ＼ の単独、／″ のみは くの字点ではない。
    #[test]
    fn test_scan_kunoji_ignores_incomplete_forms() {
        let mut n = NotationState::default();
        n.scan_kunoji("／だけ、＼だけ、／″だけ");
        assert!(!n.has_kunoji());
        assert!(!n.has_dakuten_kunoji());
    }

    /// 同じ外字名は1行にまとめ、出現箇所を順に並べる。
    #[test]
    fn test_add_unconverted_gaiji_merges_by_name() {
        let mut n = NotationState::default();
        n.add_unconverted_gaiji("「こざとへん＋井」、U+9631、133-8", true);
        n.add_unconverted_gaiji("「こざとへん＋井」、U+9631、140-2", true);
        assert_eq!(n.unconverted_gaiji().len(), 1);
        assert_eq!(
            n.unconverted_gaiji()[0].gaiji_name,
            "「こざとへん＋井」、U+9631"
        );
        assert_eq!(n.unconverted_gaiji()[0].page_lines, ["133-8", "140-2"]);
    }

    /// ＃無しの外字は名前・出現箇所とも空（参照 PAT_GAIJI の ＃ 必須）。
    #[test]
    fn test_add_unconverted_gaiji_without_igeta_is_blank() {
        let mut n = NotationState::default();
        n.add_unconverted_gaiji("「こざとへん＋井」、133-8", false);
        assert_eq!(n.unconverted_gaiji()[0].gaiji_name, "");
        assert_eq!(n.unconverted_gaiji()[0].page_lines, [""]);
    }
}
