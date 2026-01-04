//! 前方参照解決
//!
//! 青空文庫形式の「〇〇」に傍点 のようなパターンを解決します。
//! これらのコマンドは前方のテキストを参照し、装飾を適用します。

use crate::node::{
    BlockType, FontSizeType, MidashiLevel, MidashiStyle, Node, RubyDirection, StyleType,
};
use crate::parser::ruby_parser::extract_ruby_base_from_nodes;
use crate::tokenizer::tokenize;

/// ノード列の前方参照を解決
///
/// ルビの親文字抽出と、「〇〇」に傍点 形式の装飾コマンドを解決します。
pub fn resolve_references(nodes: &mut Vec<Node>) {
    // 1. ルビの親文字を解決
    resolve_ruby_bases(nodes);

    // 2. 注記付き範囲を解決（BlockStart/BlockEnd → Ruby）
    resolve_annotation_ranges(nodes);

    // 3. 装飾の前方参照を解決
    resolve_style_references(nodes);
}

/// 行内でのルビ親文字解決
///
/// 「漢字《かんじ》」形式のルビの親文字を解決します。
/// 外字ノードも漢字として親文字に含めます。
pub fn resolve_inline_ruby(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        if let Node::Ruby {
            children,
            ruby,
            direction,
        } = &nodes[i]
        {
            if children.is_empty() && !ruby.is_empty() && i > 0 {
                let ruby_clone = ruby.clone();
                let direction_clone = *direction;

                // 直前のノード列から親文字を抽出（外字も含む）
                let preceding_nodes: Vec<Node> = nodes[..i].to_vec();
                if let Some((remaining, base)) = extract_ruby_base_from_nodes(&preceding_nodes) {
                    // 残りのノード数を計算
                    let nodes_to_remove = preceding_nodes.len() - remaining.len();

                    // 前半を残りのノードで置き換え
                    let start_idx = i - nodes_to_remove;
                    nodes.splice(start_idx..i, std::iter::empty());

                    // 新しいインデックスを計算
                    let new_i = start_idx;

                    // 前半部分を挿入
                    nodes.splice(..new_i, remaining.into_iter());

                    // Rubyノードを更新（インデックスが変わっているので再計算）
                    let ruby_idx = nodes
                        .iter()
                        .position(|n| matches!(n, Node::Ruby { children: c, .. } if c.is_empty()));

                    if let Some(idx) = ruby_idx {
                        nodes[idx] = Node::Ruby {
                            children: base,
                            ruby: ruby_clone,
                            direction: direction_clone,
                        };
                    }
                    continue; // iを増やさない（ノードを操作したので）
                }
            }
        }
        i += 1;
    }
}

/// ルビの親文字を解決
fn resolve_ruby_bases(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        // 親文字が空のRubyノードを探す
        if let Node::Ruby {
            children,
            ruby,
            direction: _,
        } = &nodes[i]
        {
            if children.is_empty() && !ruby.is_empty() {
                // 直前のノードから親文字を抽出
                if i > 0 {
                    let preceding_nodes: Vec<Node> = nodes[..i].to_vec();
                    if let Some((remaining, base)) = extract_ruby_base_from_nodes(&preceding_nodes)
                    {
                        // 直前のノードを更新
                        let to_remove = i - (preceding_nodes.len() - remaining.len());

                        // 残りのノードで前半を置き換え
                        nodes.splice(..i, remaining.into_iter());

                        // 新しいインデックスを計算
                        let new_i = nodes.len() - (nodes.len() - to_remove);

                        // Rubyノードを更新
                        if let Some(Node::Ruby { children: c, .. }) = nodes.get_mut(new_i) {
                            *c = base;
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

/// 注記付き範囲を解決（BlockStart/BlockEnd → Ruby）
///
/// `［＃注記付き］内容［＃「注記」の注記付き終わり］` を `<ruby><rb>内容</rb><rt>注記</rt></ruby>` に変換
fn resolve_annotation_ranges(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        // 注記付き範囲の開始を探す
        if let Node::BlockStart { block_type, .. } = &nodes[i] {
            if *block_type == BlockType::AnnotationRange
                || *block_type == BlockType::LeftAnnotationRange
            {
                let is_left = *block_type == BlockType::LeftAnnotationRange;

                // 対応する終了を探す
                let mut end_idx = None;
                let mut annotation = None;
                for j in (i + 1)..nodes.len() {
                    if let Node::BlockEnd {
                        block_type: bt,
                        params,
                    } = &nodes[j]
                    {
                        if (*bt == BlockType::AnnotationRange && !is_left)
                            || (*bt == BlockType::LeftAnnotationRange && is_left)
                        {
                            end_idx = Some(j);
                            annotation = params.annotation.clone();
                            break;
                        }
                    }
                }

                if let (Some(end_idx), Some(annotation)) = (end_idx, annotation) {
                    // 開始から終了までの間のノードを収集
                    let children: Vec<Node> = nodes[(i + 1)..end_idx].to_vec();
                    // 注記テキストをパース（外字を含む場合があるため）
                    let annotation_nodes = parse_annotation_text(&annotation);

                    if is_left {
                        // 左注記の場合は注記として出力（Ruby版と同様）
                        // 開始マーカー + 内容ノード + 終了マーカー（外字を含む）
                        let mut new_nodes = Vec::new();
                        new_nodes.push(Node::Note("左に注記付き".to_string()));
                        new_nodes.extend(children);
                        // 終了マーカーは外字を含む可能性があるのでAnnotationEndノードを使用
                        new_nodes.push(Node::AnnotationEnd {
                            prefix: "左に「".to_string(),
                            content: annotation_nodes,
                            suffix: "」の注記付き終わり".to_string(),
                        });

                        // 範囲を新しいノード列で置き換え
                        nodes.splice(i..=end_idx, new_nodes.into_iter());
                    } else {
                        // 通常の注記付きはRubyとして出力
                        let new_node = Node::Ruby {
                            children,
                            ruby: annotation_nodes,
                            direction: RubyDirection::Right,
                        };
                        // 範囲を新しいノードで置き換え
                        nodes.splice(i..=end_idx, std::iter::once(new_node));
                    }
                    // iを増やさない（置き換えたので次のノードは同じインデックス）
                    continue;
                }
            }
        }
        i += 1;
    }
}

/// 装飾の前方参照を解決
fn resolve_style_references(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        if let Node::UnresolvedReference {
            target,
            spec,
            connector,
        } = &nodes[i]
        {
            let target_clone = target.clone();
            let spec_clone = spec.clone();
            let connector_clone = connector.clone();

            // 前方のノードから対象テキストを探す
            if let Some((_, found_node_idx, split_info)) =
                find_target_in_preceding(&nodes[..i], &target_clone)
            {
                // 解決種類を決定
                if let Some(kind) = ResolvedKind::from_spec(&spec_clone) {
                    apply_resolution(
                        nodes,
                        &mut i,
                        found_node_idx,
                        split_info,
                        &target_clone,
                        &kind,
                    );
                    continue;
                }
            }

            // 解決できなかった場合はNoteノードに変換
            nodes[i] = Node::Note(format!("「{target_clone}」{connector_clone}{spec_clone}"));
        }
        i += 1;
    }
}

/// 解決結果をノード列に適用
fn apply_resolution(
    nodes: &mut Vec<Node>,
    i: &mut usize,
    found_node_idx: usize,
    split_info: SplitInfo,
    target: &str,
    kind: &ResolvedKind,
) {
    match split_info {
        SplitInfo::ExactMatch => {
            let new_node = kind.create_node(target);
            nodes[found_node_idx] = new_node;
            nodes.remove(*i);
        }
        SplitInfo::Split { before, after } => {
            let new_node = kind.create_node(target);
            let mut new_nodes = Vec::new();
            if !before.is_empty() {
                new_nodes.push(Node::text(&before));
            }
            new_nodes.push(new_node);
            if !after.is_empty() {
                new_nodes.push(Node::text(&after));
            }
            nodes.splice(found_node_idx..found_node_idx + 1, new_nodes.into_iter());
            let adjustment =
                if before.is_empty() { 0 } else { 1 } + if after.is_empty() { 0 } else { 1 };
            let new_i = *i + adjustment;
            if new_i < nodes.len() {
                nodes.remove(new_i);
            }
        }
        SplitInfo::MultiNodeExact { start_idx, end_idx } => {
            let children: Vec<Node> = nodes[start_idx..=end_idx].to_vec();
            let new_node = kind.create_node_with_children(children);
            let nodes_removed = end_idx - start_idx + 1;
            nodes.splice(start_idx..=end_idx, std::iter::once(new_node));
            let new_i = *i - (nodes_removed - 1);
            if new_i < nodes.len() {
                nodes.remove(new_i);
            }
        }
    }
}

/// 前方のノードから対象テキストを探す
fn find_target_in_preceding(nodes: &[Node], target: &str) -> Option<(usize, usize, SplitInfo)> {
    // まず単一ノード内で探す（後ろから）
    for (i, node) in nodes.iter().enumerate().rev() {
        match node {
            Node::Text(text) => {
                if text == target {
                    return Some((i, i, SplitInfo::ExactMatch));
                }
                // 末尾から検索（同じ文字が連続する場合、後のものを優先）
                if let Some(pos) = text.rfind(target) {
                    let before = text[..pos].to_string();
                    let after = text[pos + target.len()..].to_string();
                    return Some((i, i, SplitInfo::Split { before, after }));
                }
            }
            // 子を持つノードの場合、内容テキストが完全一致するかチェック
            Node::FontSize { .. }
            | Node::Style { .. }
            | Node::Tcy { .. }
            | Node::Keigakomi { .. }
            | Node::Yokogumi { .. }
            | Node::Caption { .. }
            | Node::Midashi { .. } => {
                let content = extract_plain_text(node);
                if content == target {
                    // ノード全体をラップ対象として返す
                    return Some((
                        i,
                        i,
                        SplitInfo::MultiNodeExact {
                            start_idx: i,
                            end_idx: i,
                        },
                    ));
                }
            }
            _ => {}
        }
    }

    // 複数ノードにまたがる場合を探す
    // ノード列の末尾から連続したノードのプレーンテキストを結合して探す
    for end_idx in (0..nodes.len()).rev() {
        let mut combined = String::new();

        // 末尾から連結していく
        for start_idx in (0..=end_idx).rev() {
            let text = extract_plain_text(&nodes[start_idx]);
            combined = format!("{}{}", text, combined);

            // 対象テキストが含まれていれば
            if combined.contains(target) {
                // 完全一致（連結テキスト == 対象）かチェック
                if combined == target {
                    return Some((
                        start_idx,
                        end_idx,
                        SplitInfo::MultiNodeExact { start_idx, end_idx },
                    ));
                }
                // 部分一致の場合、対象がノード境界に一致しているかチェック
                if combined.ends_with(target) {
                    // 末尾一致：前半のノードを分割する必要があるかも
                    let prefix_len = combined.len() - target.len();
                    if prefix_len == 0 {
                        return Some((
                            start_idx,
                            end_idx,
                            SplitInfo::MultiNodeExact { start_idx, end_idx },
                        ));
                    }
                }
            }
        }
    }

    None
}

/// ノードからプレーンテキストを抽出
fn extract_plain_text(node: &Node) -> String {
    match node {
        Node::Text(text) => text.clone(),
        Node::Ruby { children, .. } => {
            // Rubyノードからは親文字のみ抽出
            children.iter().map(extract_plain_text).collect()
        }
        Node::Style { children, .. } => children.iter().map(extract_plain_text).collect(),
        Node::FontSize { children, .. } => children.iter().map(extract_plain_text).collect(),
        Node::Tcy { children } => children.iter().map(extract_plain_text).collect(),
        Node::Keigakomi { children } => children.iter().map(extract_plain_text).collect(),
        Node::Yokogumi { children } => children.iter().map(extract_plain_text).collect(),
        Node::Caption { children } => children.iter().map(extract_plain_text).collect(),
        Node::Midashi { children, .. } => children.iter().map(extract_plain_text).collect(),
        _ => String::new(),
    }
}

/// 解決された参照の種類
#[derive(Debug, Clone)]
enum ResolvedKind {
    /// スタイル（傍点、傍線など）
    Style(StyleType),
    /// 見出し
    Midashi {
        level: MidashiLevel,
        style: MidashiStyle,
    },
    /// フォントサイズ
    FontSize { size_type: FontSizeType, level: u32 },
    /// インライン要素（縦中横、罫囲み、横組み、キャプション）
    Inline(InlineKind),
    /// 注記ルビ
    AnnotationRuby { annotation: String },
    /// 傍記（ルビとして表示）
    SideNote { annotation: String },
}

impl ResolvedKind {
    /// 参照スペックを解析して解決された種類を返す
    fn from_spec(spec: &str) -> Option<Self> {
        // 注記ルビ（annotation_ruby:注記内容）
        if let Some(annotation) = spec.strip_prefix("annotation_ruby:") {
            return Some(ResolvedKind::AnnotationRuby {
                annotation: annotation.to_string(),
            });
        }

        // 傍記（side_note:注記内容）
        if let Some(annotation) = spec.strip_prefix("side_note:") {
            return Some(ResolvedKind::SideNote {
                annotation: annotation.to_string(),
            });
        }

        // スタイル
        if let Some(style_type) = StyleType::from_command(spec) {
            return Some(ResolvedKind::Style(style_type));
        }

        // 見出し
        if let Some(level) = MidashiLevel::from_command(spec) {
            let style = MidashiStyle::from_command(spec);
            return Some(ResolvedKind::Midashi { level, style });
        }

        // フォントサイズ
        if let Some((size_type, level)) = FontSizeType::from_command(spec) {
            return Some(ResolvedKind::FontSize { size_type, level });
        }

        // インライン要素
        if let Some(inline_kind) = InlineKind::from_spec(spec) {
            return Some(ResolvedKind::Inline(inline_kind));
        }

        None
    }

    /// 対象テキストからノードを作成
    fn create_node(&self, target: &str) -> Node {
        self.create_node_with_children(vec![Node::text(target)])
    }

    /// 子ノード列からノードを作成
    fn create_node_with_children(&self, children: Vec<Node>) -> Node {
        match self {
            ResolvedKind::Style(style_type) => Node::Style {
                children,
                style_type: *style_type,
                class_name: String::new(),
            },
            ResolvedKind::Midashi { level, style } => Node::Midashi {
                children,
                level: *level,
                style: *style,
            },
            ResolvedKind::FontSize { size_type, level } => Node::FontSize {
                children,
                size_type: *size_type,
                level: *level,
            },
            ResolvedKind::Inline(inline_kind) => inline_kind.create_node(children),
            ResolvedKind::AnnotationRuby { annotation } => Node::Ruby {
                children,
                ruby: vec![Node::text(annotation)],
                direction: RubyDirection::Right,
            },
            ResolvedKind::SideNote { annotation } => {
                // 親文字の文字数を数える
                let char_count: usize = children.iter().map(|n| n.to_text().chars().count()).sum();
                // 注記を文字数分繰り返し、&nbsp;で区切る
                let repeated: String = std::iter::repeat(annotation.as_str())
                    .take(char_count.max(1))
                    .collect::<Vec<_>>()
                    .join("\u{00a0}"); // non-breaking space
                Node::Ruby {
                    children,
                    ruby: vec![Node::text(&repeated)],
                    direction: RubyDirection::Right,
                }
            }
        }
    }
}

/// インライン要素の種類
#[derive(Debug, Clone, Copy)]
enum InlineKind {
    Tcy,
    Keigakomi,
    Yokogumi,
    Caption,
}

impl InlineKind {
    /// スペック文字列からインライン種類を取得
    fn from_spec(spec: &str) -> Option<Self> {
        match spec {
            "縦中横" => Some(InlineKind::Tcy),
            "罫囲み" => Some(InlineKind::Keigakomi),
            "横組み" => Some(InlineKind::Yokogumi),
            "キャプション" => Some(InlineKind::Caption),
            _ => None,
        }
    }

    /// 子ノード列からノードを作成
    fn create_node(self, children: Vec<Node>) -> Node {
        match self {
            InlineKind::Tcy => Node::Tcy { children },
            InlineKind::Keigakomi => Node::Keigakomi { children },
            InlineKind::Yokogumi => Node::Yokogumi { children },
            InlineKind::Caption => Node::Caption { children },
        }
    }
}

/// 分割情報
enum SplitInfo {
    /// 完全一致
    ExactMatch,
    /// 分割が必要
    Split { before: String, after: String },
    /// 複数ノードにまたがる完全一致
    MultiNodeExact { start_idx: usize, end_idx: usize },
}

/// 注記テキストをノード列にパース
///
/// 外字表記（`※［＃...］`）を含むテキストをパースして、
/// テキストノードと外字ノードの列に変換します。
fn parse_annotation_text(text: &str) -> Vec<Node> {
    use crate::gaiji::{parse_gaiji, GaijiResult};
    use crate::token::Token;

    let tokens = tokenize(text);
    let mut nodes = Vec::new();

    for token in tokens {
        match token {
            Token::Text(s) => nodes.push(Node::text(&s)),
            Token::Gaiji { description } => {
                let node = match parse_gaiji(&description) {
                    GaijiResult::Unicode(s) => Node::Gaiji {
                        description: description.clone(),
                        unicode: Some(s),
                        jis_code: None,
                    },
                    GaijiResult::JisConverted { jis_code, unicode } => Node::Gaiji {
                        description: description.clone(),
                        unicode: Some(unicode),
                        jis_code: Some(jis_code),
                    },
                    GaijiResult::JisImage { jis_code } => Node::Gaiji {
                        description: description.clone(),
                        unicode: None,
                        jis_code: Some(jis_code),
                    },
                    GaijiResult::Unconvertible => Node::Gaiji {
                        description: description.clone(),
                        unicode: None,
                        jis_code: None,
                    },
                };
                nodes.push(node);
            }
            // その他のトークンは無視（注記内にはルビやコマンドは含まれない想定）
            _ => {}
        }
    }

    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::RubyDirection;

    #[test]
    fn test_resolve_inline_ruby() {
        let mut nodes = vec![
            Node::text("私の東京"),
            Node::Ruby {
                children: vec![],
                ruby: vec![Node::text("とうきょう")],
                direction: RubyDirection::Right,
            },
        ];

        resolve_inline_ruby(&mut nodes);

        assert_eq!(nodes.len(), 2);
        assert!(matches!(&nodes[0], Node::Text(s) if s == "私の"));
        if let Node::Ruby { children, ruby, .. } = &nodes[1] {
            assert!(matches!(&children[0], Node::Text(s) if s == "東京"));
            assert!(matches!(&ruby[0], Node::Text(s) if s == "とうきょう"));
        } else {
            panic!("Expected Ruby node");
        }
    }

    #[test]
    fn test_resolve_inline_ruby_full_match() {
        let mut nodes = vec![
            Node::text("東京"),
            Node::Ruby {
                children: vec![],
                ruby: vec![Node::text("とうきょう")],
                direction: RubyDirection::Right,
            },
        ];

        resolve_inline_ruby(&mut nodes);

        assert_eq!(nodes.len(), 1);
        if let Node::Ruby { children, ruby, .. } = &nodes[0] {
            assert!(matches!(&children[0], Node::Text(s) if s == "東京"));
            assert!(matches!(&ruby[0], Node::Text(s) if s == "とうきょう"));
        } else {
            panic!("Expected Ruby node");
        }
    }

    #[test]
    fn test_resolve_style_reference() {
        let mut nodes = vec![
            Node::text("重要なこと"),
            Node::UnresolvedReference {
                target: "重要".to_string(),
                spec: "sesame_dot".to_string(),
                connector: "に".to_string(),
            },
        ];

        resolve_style_references(&mut nodes);

        // 「重要」が装飾ノードになっているはず
        assert!(!nodes.is_empty());
    }

    #[test]
    fn test_find_target_exact() {
        let nodes = vec![
            Node::text("前の文"),
            Node::text("重要"),
            Node::text("後の文"),
        ];

        let result = find_target_in_preceding(&nodes, "重要");
        assert!(result.is_some());
        let (_, idx, split) = result.unwrap();
        assert_eq!(idx, 1);
        assert!(matches!(split, SplitInfo::ExactMatch));
    }

    #[test]
    fn test_find_target_split() {
        let nodes = vec![Node::text("これは重要なことだ")];

        let result = find_target_in_preceding(&nodes, "重要");
        assert!(result.is_some());
        let (_, idx, split) = result.unwrap();
        assert_eq!(idx, 0);
        if let SplitInfo::Split { before, after } = split {
            assert_eq!(before, "これは");
            assert_eq!(after, "なことだ");
        } else {
            panic!("Expected Split");
        }
    }
}
