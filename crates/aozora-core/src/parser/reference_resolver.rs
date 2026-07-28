//! 前方参照解決
//!
//! 青空文庫形式の「〇〇」に傍点 のようなパターンを解決します。
//! これらのコマンドは前方のテキストを参照し、装飾を適用します。

use crate::node::{BlockType, Node, NodeKind, RefSpec, RubyDirection};
use crate::parser::ruby_parser::extract_ruby_base_from_nodes;
use crate::token::Span;
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

/// ルビ親文字解決の 2 パス目。中身は `resolve_ruby_bases` と同一で、呼ぶ位置だけが違う。
/// 呼び出し元は [`resolve_references`] の直後にこれを呼ぶ。
///
/// 2 パス必要な理由は両方向ともテストが固定している:
/// `stage3_needs_ruby_bases_resolved`（前を省けない）/
/// `stage4_resolves_bases_that_appear_only_after_stage3`（後を省けない）。
pub fn resolve_inline_ruby(nodes: &mut Vec<Node>) {
    resolve_ruby_bases(nodes);
}

/// ルビの親文字を解決（工程1・工程4 共通の実体）
fn resolve_ruby_bases(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        // 親文字が空の Ruby ノードだけが対象。行頭（i == 0）なら取り込む前方が無い。
        let needs_base = matches!(
            &nodes[i].kind,
            NodeKind::Ruby { children, ruby, .. } if children.is_empty() && !ruby.is_empty()
        );
        if !needs_base || i == 0 {
            i += 1;
            continue;
        }

        // 直前のノード列から親文字を抽出する。
        let Some((remaining, base)) = extract_ruby_base_from_nodes(&nodes[..i]) else {
            i += 1;
            continue;
        };

        // 前半を「親文字を取り除いた残り」で置き換える。ノード列はここで
        // i - remaining.len() 個縮み、Ruby ノードは remaining.len() の位置へ動く。
        let ruby_idx = remaining.len();
        nodes.splice(..i, remaining);

        if let Node {
            kind: NodeKind::Ruby { children, .. },
            span,
            ..
        } = &mut nodes[ruby_idx]
        {
            *span = base.iter().fold(*span, |span, node| span.union(node.span));
            *children = base;
        }

        // 縮んだ分を反映して Ruby の次から走査を続ける。旧コードは i をそのまま
        // 進めていたため、縮んだ個数だけ後続ノードを読み飛ばしていた
        // （例:「あ《い》《え》」の 2 つ目のルビが未訪問のまま残る）。
        i = ruby_idx + 1;
    }
}

/// 注記付き範囲を解決（BlockStart/BlockEnd → Ruby）
///
/// `［＃注記付き］内容［＃「注記」の注記付き終わり］` を `<ruby><rb>内容</rb><rt>注記</rt></ruby>` に変換
fn resolve_annotation_ranges(nodes: &mut Vec<Node>) {
    let mut i = 0;
    while i < nodes.len() {
        // 注記付き範囲の開始を探す
        if let NodeKind::BlockStart { block_type, .. } = &nodes[i].kind {
            if *block_type == BlockType::AnnotationRange
                || *block_type == BlockType::LeftAnnotationRange
            {
                let is_left = *block_type == BlockType::LeftAnnotationRange;

                // 対応する終了を探す
                let mut end_idx = None;
                let mut annotation = None;
                for j in (i + 1)..nodes.len() {
                    if let NodeKind::BlockEnd {
                        block_type: bt,
                        params,
                        ..
                    } = &nodes[j].kind
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
                    let range = nodes[i].span.union(nodes[end_idx].span);
                    let annotation_nodes = parse_annotation_text(&annotation, nodes[end_idx].span);

                    if is_left {
                        // 左注記の場合は注記として出力（Ruby版と同様）
                        // 開始マーカー + 内容ノード + 終了マーカー（外字を含む）
                        let mut new_nodes = Vec::new();
                        new_nodes.push(Node::new(
                            NodeKind::Note("左に注記付き".to_string()),
                            nodes[i].span,
                        ));
                        new_nodes.extend(children);
                        // 終了マーカーは外字を含む可能性があるのでAnnotationEndノードを使用
                        new_nodes.push(Node::new(
                            NodeKind::AnnotationEnd {
                                prefix: "左に「".to_string(),
                                content: annotation_nodes,
                                suffix: "」の注記付き終わり".to_string(),
                            },
                            nodes[end_idx].span,
                        ));

                        // 範囲を新しいノード列で置き換え
                        nodes.splice(i..=end_idx, new_nodes.into_iter());
                    } else {
                        // 通常の注記付きはRubyとして出力
                        let new_node = Node::new(
                            NodeKind::Ruby {
                                children,
                                ruby: annotation_nodes,
                                direction: RubyDirection::Right,
                                keep_gaiji_notes_in_base: true,
                            },
                            range,
                        );
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
/// 前方参照を解決しつつ、解決できず注記化した参照の `raw` 文字列を返す。
/// ノード変換の結果は [`resolve_references`] と完全に同一で、失敗リストを追加返却するだけ。
/// エディタ支援 `analysis` が「実際に解決できなかった参照」を **1 行 1 回**の解決で
/// 知るために使う（参照ごとに resolve をやり直す二次的コストを避ける）。
pub fn resolve_references_collecting_failures(nodes: &mut Vec<Node>) -> Vec<String> {
    resolve_ruby_bases(nodes);
    resolve_annotation_ranges(nodes);
    let mut failed = Vec::new();
    resolve_style_references_collecting(nodes, &mut failed);
    failed
}

fn resolve_style_references(nodes: &mut Vec<Node>) {
    resolve_style_references_collecting(nodes, &mut Vec::new());
}

/// [`resolve_style_references`] と同じ解決を行い、解決できず注記化したトップレベル参照の
/// `raw` を `failed` に集める（子ノード内の失敗は集めない）。
fn resolve_style_references_collecting(nodes: &mut Vec<Node>, failed: &mut Vec<String>) {
    let mut i = 0;
    while i < nodes.len() {
        if let NodeKind::UnresolvedReference { target, spec, raw } = &nodes[i].kind {
            let target_clone = target.clone();
            let spec_clone = spec.clone();
            let raw_clone = raw.clone();

            // 参照実装 search_front_reference を移植: 直前ノード（バッファ末尾）
            // から連続した接尾辞スパンとして対象を探す。i == 0 なら前が無い。
            if i > 0 {
                if let Some(m) = search_front_reference(&nodes[..i], i - 1, &target_clone) {
                    apply_front_reference(nodes, &mut i, m, &spec_clone);
                    continue;
                }
            }

            // 解決できなかった場合は、もとの文字列のまま注記にする
            failed.push(raw_clone.clone());
            let span = nodes[i].span;
            nodes[i] = Node::new(NodeKind::Note(raw_clone), span);
        }
        i += 1;
    }

    // 子ノードの中の前方参照も解決する。参照実装は ｜瀕［＃「瀕」は太字］《ひん》
    // のようにルビ親文字の内側で前方参照を解決してから（@ruby_buf を対象に）
    // ルビを閉じる。こちらはルビ親文字解決後、親文字ノード列の中に前方参照が
    // 残るので、各コンテナの子へ再帰して同じスコープ内で解決する。
    for node in nodes.iter_mut() {
        resolve_style_references_in_children(node);
    }
}

/// コンテナノードの子ノード列に対して前方参照解決を再帰的に適用する。
fn resolve_style_references_in_children(node: &mut Node) {
    for children in node.kind.inline_child_lists_mut() {
        resolve_style_references(children);
    }
}

/// 前方参照の照合結果。start_idx..=（注記の直前）を子ノードに置き換える。
struct FrontRefMatch {
    /// 消費する最古ノードの位置
    start_idx: usize,
    /// 最古ノードが Text 分割だったときにバッファへ残す前半（それ以外は空）
    prefix: String,
    /// ラップ対象のノード列（古い順）
    children: Vec<Node>,
}

/// 参照実装 search_front_reference の移植。
///
/// バッファ末尾（end_idx）から連続した要素を消費して、対象 `target` を接尾辞
/// スパンとして照合する。String 要素は末尾一致なら分割（前半を残す）、対象が
/// その要素で終わるなら残りを前方へ再帰。ReferenceMentioned 要素は内部テキストが
/// 対象末尾に一致するなら要素まるごとを子に取り込む。空文字列要素は読み飛ばす。
/// それ以外（画像・外字など）に当たった時点で照合失敗。
fn search_front_reference(nodes: &[Node], end_idx: usize, target: &str) -> Option<FrontRefMatch> {
    if target.is_empty() {
        return None;
    }
    match &nodes[end_idx].kind {
        NodeKind::Text(s) => {
            if s.is_empty() {
                // 空文字列は捨てて同じ対象で1つ前へ。
                return end_idx
                    .checked_sub(1)
                    .and_then(|e| search_front_reference(nodes, e, target));
            }
            // 完全一致: s が対象で終わる → s を分割し前半を残す。
            if let Some(prefix) = s.strip_suffix(target) {
                return Some(FrontRefMatch {
                    start_idx: end_idx,
                    prefix: prefix.to_string(),
                    children: vec![Node::text(
                        target,
                        nodes[end_idx].span.split_at(prefix.chars().count()).1,
                    )],
                });
            }
            // 部分一致: 対象が s で終わる → s は対象の末尾セグメント。残りを再帰。
            if let Some(remaining) = target.strip_suffix(s.as_str()) {
                let e = end_idx.checked_sub(1)?;
                let mut sub = search_front_reference(nodes, e, remaining)?;
                sub.children.push(Node::text(s, nodes[end_idx].span));
                return Some(sub);
            }
            None
        }
        // 参照実装 ReferenceMentioned 相当のインラインコンテナ。
        kind if kind.inline_container_children().is_some() => {
            let node = &nodes[end_idx];
            let inner = extract_plain_text(node);
            if inner.is_empty() {
                return None;
            }
            if inner == target {
                return Some(FrontRefMatch {
                    start_idx: end_idx,
                    prefix: String::new(),
                    children: vec![node.clone()],
                });
            }
            if let Some(remaining) = target.strip_suffix(inner.as_str()) {
                let e = end_idx.checked_sub(1)?;
                let mut sub = search_front_reference(nodes, e, remaining)?;
                sub.children.push(node.clone());
                return Some(sub);
            }
            None
        }
        _ => None,
    }
}

/// 照合結果をノード列に適用する。start_idx..=（注記の直前）を
/// [分割前半?][解決済みノード] に置き換え、注記自身を除去する。
fn apply_front_reference(nodes: &mut Vec<Node>, i: &mut usize, m: FrontRefMatch, spec: &RefSpec) {
    let reference_span = nodes[*i].span;
    let combined_span = m
        .children
        .iter()
        .fold(reference_span, |span, node| span.union(node.span));
    let new_node = spec.resolve(m.children, combined_span);
    let mut replacement = Vec::new();
    if !m.prefix.is_empty() {
        let prefix_span = nodes[m.start_idx].span.split_at(m.prefix.chars().count()).0;
        replacement.push(Node::text(&m.prefix, prefix_span));
    }
    replacement.push(new_node);
    let r = replacement.len();

    // 注記の直前（*i - 1）までがスパン。start_idx..=(*i-1) を置き換える。
    let end = *i - 1;
    nodes.splice(m.start_idx..=end, replacement);

    // 置換後、注記は start_idx + r の位置へ移動している。除去して、続きは
    // 注記の次のノードから（continue で再検査）。
    let annotation_idx = m.start_idx + r;
    if annotation_idx < nodes.len() {
        nodes.remove(annotation_idx);
    }
    *i = annotation_idx;
}

/// ノードからプレーンテキストを抽出（前方参照の照合に使う）。
///
/// インラインコンテナは照合対象の子（ルビなら親文字のみ）を再帰的にたどる。
/// それ以外——外字・画像・アクセント・訓点など——は空文字列になり、照合は失敗する。
fn extract_plain_text(node: &Node) -> String {
    match &node.kind {
        NodeKind::Text(text) => text.clone(),
        kind => kind
            .inline_container_children()
            .map(|children| children.iter().map(extract_plain_text).collect())
            .unwrap_or_default(),
    }
}

/// 注記テキストをノード列にパース
///
/// 外字表記（`※［＃...］`）を含むテキストをパースして、
/// テキストノードと外字ノードの列に変換します。
fn parse_annotation_text(text: &str, span: Span) -> Vec<Node> {
    use crate::gaiji::{parse_gaiji, GaijiResult};
    use crate::token::TokenKind;

    let tokens = tokenize(text);
    let mut nodes = Vec::new();

    for token in tokens {
        match token.kind {
            TokenKind::Text(s) => nodes.push(Node::text(&s, span)),
            TokenKind::Gaiji {
                description,
                had_igeta,
            } => {
                let node = match parse_gaiji(&description) {
                    GaijiResult::Unicode(s) => NodeKind::Gaiji {
                        description: description.clone(),
                        unicode: Some(s),
                        jis_code: None,
                        had_igeta,
                    },
                    GaijiResult::JisConverted { jis_code, unicode } => NodeKind::Gaiji {
                        description: description.clone(),
                        unicode: Some(unicode),
                        jis_code: Some(jis_code),
                        had_igeta,
                    },
                    GaijiResult::JisImage { jis_code } => NodeKind::Gaiji {
                        description: description.clone(),
                        unicode: None,
                        jis_code: Some(jis_code),
                        had_igeta,
                    },
                    GaijiResult::Unconvertible => NodeKind::Gaiji {
                        description: description.clone(),
                        unicode: None,
                        jis_code: None,
                        had_igeta,
                    },
                };
                nodes.push(Node::new(node, span));
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
    use crate::node::{InlineKind, RubyDirection, StyleType};

    fn text(value: &str) -> Node {
        Node::text(value, Span::new(0, value.chars().count()))
    }

    fn node(kind: NodeKind) -> Node {
        Node::new(kind, Span::new(0, 0))
    }

    #[test]
    fn test_resolve_inline_ruby() {
        let mut nodes = vec![
            text("私の東京"),
            node(NodeKind::Ruby {
                children: vec![],
                ruby: vec![text("とうきょう")],
                direction: RubyDirection::Right,
                keep_gaiji_notes_in_base: false,
            }),
        ];

        resolve_inline_ruby(&mut nodes);

        assert_eq!(nodes.len(), 2);
        assert!(matches!(&nodes[0].kind, NodeKind::Text(s) if s == "私の"));
        if let NodeKind::Ruby { children, ruby, .. } = &nodes[1].kind {
            assert!(matches!(&children[0].kind, NodeKind::Text(s) if s == "東京"));
            assert!(matches!(&ruby[0].kind, NodeKind::Text(s) if s == "とうきょう"));
        } else {
            panic!("Expected Ruby node");
        }
    }

    #[test]
    fn test_resolve_inline_ruby_full_match() {
        let mut nodes = vec![
            text("東京"),
            node(NodeKind::Ruby {
                children: vec![],
                ruby: vec![text("とうきょう")],
                direction: RubyDirection::Right,
                keep_gaiji_notes_in_base: false,
            }),
        ];

        resolve_inline_ruby(&mut nodes);

        assert_eq!(nodes.len(), 1);
        if let NodeKind::Ruby { children, ruby, .. } = &nodes[0].kind {
            assert!(matches!(&children[0].kind, NodeKind::Text(s) if s == "東京"));
            assert!(matches!(&ruby[0].kind, NodeKind::Text(s) if s == "とうきょう"));
        } else {
            panic!("Expected Ruby node");
        }
    }

    #[test]
    fn stage3_needs_ruby_bases_resolved() {
        // 前方参照の照合は Ruby の内部テキスト＝親文字（children）を見る。親文字が
        // 空のままでは照合できないので、参照解決より前に親文字解決を走らせる必要がある。
        let pending = node(NodeKind::Ruby {
            children: vec![],
            ruby: vec![text("とうきょう")],
            direction: RubyDirection::Right,
            keep_gaiji_notes_in_base: false,
        });
        assert!(
            search_front_reference(&[text("東京"), pending], 1, "東京").is_none(),
            "親文字が空のルビは照合対象にならない"
        );

        // 親文字が入っていれば、ルビ 1 ノードを丸ごと対象にできる。
        let resolved = node(NodeKind::Ruby {
            children: vec![text("東京")],
            ruby: vec![text("とうきょう")],
            direction: RubyDirection::Right,
            keep_gaiji_notes_in_base: false,
        });
        let m =
            search_front_reference(&[resolved], 0, "東京").expect("親文字が入っていれば照合できる");
        assert!(matches!(
            &m.children[..],
            [Node {
                kind: NodeKind::Ruby { .. },
                ..
            }]
        ));
    }

    #[test]
    fn stage4_resolves_bases_that_appear_only_after_stage3() {
        use crate::parser::parse_raw_nodes;
        use crate::tokenizer::tokenize;

        // 直前が未解決参照だと last_char_type() が None を返すため、1 パス目では
        // 親文字を取れない。参照が装飾タグへ解決されて初めて「タグ 1 ノードが親文字」
        // の規則が使えるようになる。
        let mut nodes = parse_raw_nodes(&tokenize(
            "公事根源［＃「公事根源」は斜体］《くじこんげん》",
        ));
        resolve_references(&mut nodes);
        let NodeKind::Ruby { children, .. } = &nodes[1].kind else {
            panic!("参照解決後は [Style, Ruby] になるはず: {nodes:?}");
        };
        assert!(
            children.is_empty(),
            "この時点ではまだ親文字が空（だから 2 パス目が要る）: {nodes:?}"
        );

        // 2 パス目が、直前に生まれた斜体タグを親文字として取り込む。
        resolve_inline_ruby(&mut nodes);
        assert_eq!(nodes.len(), 1, "{nodes:?}");
        let NodeKind::Ruby { children, .. } = &nodes[0].kind else {
            panic!("{nodes:?}");
        };
        assert!(
            matches!(&children[0].kind, NodeKind::Style { .. }),
            "斜体タグが親文字になっていない: {children:?}"
        );
    }

    #[test]
    fn stage4_does_not_overwrite_an_earlier_unresolved_ruby() {
        use crate::parser::parse_raw_nodes;
        use crate::tokenizer::tokenize;

        // 行頭の 《あ》 は直前ノードが無い（i > 0 の条件）ため親文字を取れず、
        // 親文字が空のまま最後まで残る。その状態で工程4が後続ルビ（親文字＝斜体タグ）を
        // 解決するとき、手前の空ルビを掴んで上書きしてはいけない。
        // 旧 resolve_inline_ruby は更新対象を position() で「先頭から最初の空ルビ」と
        // して探していたため、rt=あ が消えて くじこんげん が二重化していた。
        let mut nodes = parse_raw_nodes(&tokenize(
            "《あ》公事根源［＃「公事根源」は斜体］《くじこんげん》",
        ));
        resolve_references(&mut nodes);
        resolve_inline_ruby(&mut nodes);

        assert_eq!(nodes.len(), 2, "ノード数が想定外: {nodes:?}");

        // 行頭ルビは手つかず（親文字空・rt=あ）で残る。
        let NodeKind::Ruby { children, ruby, .. } = &nodes[0].kind else {
            panic!("先頭が Ruby でない: {nodes:?}");
        };
        assert!(
            children.is_empty(),
            "行頭ルビの親文字が書き換えられた: {nodes:?}"
        );
        assert!(
            matches!(&ruby[0].kind, NodeKind::Text(s) if s == "あ"),
            "行頭ルビの rt が失われた: {nodes:?}"
        );

        // 後続ルビは工程3で生まれた斜体タグを親文字として解決される。
        let NodeKind::Ruby { children, ruby, .. } = &nodes[1].kind else {
            panic!("2つ目が Ruby でない: {nodes:?}");
        };
        assert!(
            matches!(&children[0].kind, NodeKind::Style { .. }),
            "親文字が斜体タグになっていない: {children:?}"
        );
        assert!(
            matches!(&ruby[0].kind, NodeKind::Text(s) if s == "くじこんげん"),
            "rt が想定外: {ruby:?}"
        );
    }

    #[test]
    fn ruby_base_resolution_does_not_skip_later_rubies() {
        use crate::parser::parse_raw_nodes;
        use crate::tokenizer::tokenize;

        // 外字＋漢字がまるごと親文字になると、前半のノード列が 2 個縮む。旧実装は
        // splice で縮んだ分だけ走査位置を補正せず i += 1 していたため、後続の
        // 「あ《い》」が未訪問のまま（親文字が空のまま）残っていた。
        let mut nodes = parse_raw_nodes(&tokenize("※［＃「丸印」、U+25CB］東《とう》あ《い》"));
        resolve_ruby_bases(&mut nodes);

        assert_eq!(nodes.len(), 2, "ノード数が想定外: {nodes:?}");
        let NodeKind::Ruby { children, .. } = &nodes[0].kind else {
            panic!("先頭が Ruby でない: {nodes:?}");
        };
        assert_eq!(
            children.len(),
            2,
            "外字＋漢字が親文字になっていない: {nodes:?}"
        );

        let NodeKind::Ruby { children, ruby, .. } = &nodes[1].kind else {
            panic!("2つ目が Ruby でない: {nodes:?}");
        };
        assert!(
            matches!(&children[..], [Node { kind: NodeKind::Text(s), .. }] if s == "あ"),
            "2つ目のルビの親文字が解決されていない: {nodes:?}"
        );
        assert!(matches!(&ruby[0].kind, NodeKind::Text(s) if s == "い"));
    }

    #[test]
    fn test_resolve_style_reference() {
        // 参照実装は対象を直前バッファの接尾辞としてしか照合しないので、
        // 対象「重要」はテキストの末尾になければならない。
        let mut nodes = vec![
            text("とても重要"),
            node(NodeKind::UnresolvedReference {
                target: "重要".to_string(),
                spec: RefSpec::Style(StyleType::SesameDot),
                raw: "「重要」に傍点".to_string(),
            }),
        ];

        resolve_style_references(&mut nodes);

        // 「重要」が装飾ノードになっているはず
        assert!(nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Style { .. })));
    }

    #[test]
    fn test_multinode_resolution_does_not_skip_following_reference() {
        // 複数ノードにまたがる前方参照（見出し等）の解決で対象ノードが減った
        // あと、直後の未解決参照（縦中横）を読み飛ばさず解決すること。
        // 従来は MultiNodeExact 適用時に *i を更新せず、直後の参照を skip して
        // いた（同行中見出し＋日付の縦中横が注記化するバグ）。
        let mut nodes = vec![
            text("前"),
            text("半"),
            node(NodeKind::UnresolvedReference {
                target: "前半".to_string(),
                spec: RefSpec::Style(StyleType::Bold),
                raw: "「前半」は太字".to_string(),
            }),
            text("４・19"),
            node(NodeKind::UnresolvedReference {
                target: "19".to_string(),
                spec: RefSpec::Inline(InlineKind::Tcy),
                raw: "「19」は縦中横".to_string(),
            }),
        ];
        resolve_style_references(&mut nodes);
        assert!(
            nodes.iter().any(|n| matches!(n.kind, NodeKind::Tcy { .. })),
            "見出し解決の直後の縦中横参照が解決されていない: {nodes:?}"
        );
        assert!(
            !nodes
                .iter()
                .any(|n| matches!(n.kind, NodeKind::UnresolvedReference { .. })),
            "未解決参照が残っている: {nodes:?}"
        );
    }

    #[test]
    fn test_resolve_frontref_inside_ruby_base() {
        // ｜瀕［＃「瀕」は太字］《ひん》: ルビ親文字の内側に前方参照がある場合、
        // 親文字ノード列の中で解決してから <rb> に入れる（子への再帰）。
        let mut nodes = vec![node(NodeKind::Ruby {
            keep_gaiji_notes_in_base: false,
            children: vec![
                text("瀕"),
                node(NodeKind::UnresolvedReference {
                    target: "瀕".to_string(),
                    spec: RefSpec::Style(StyleType::Bold),
                    raw: "「瀕」は太字".to_string(),
                }),
            ],
            ruby: vec![text("ひん")],
            direction: RubyDirection::Right,
        })];
        resolve_style_references(&mut nodes);
        if let NodeKind::Ruby { children, .. } = &nodes[0].kind {
            assert_eq!(
                children.len(),
                1,
                "親文字が装飾1ノードに畳まれていない: {children:?}"
            );
            assert!(
                matches!(
                    &children[0].kind,
                    NodeKind::Style {
                        style_type: StyleType::Bold,
                        ..
                    }
                ),
                "親文字内の前方参照が太字に解決されていない: {children:?}"
            );
        } else {
            panic!("Ruby node が壊れた");
        }
    }

    #[test]
    fn test_search_front_reference_tail_only() {
        // 末尾（接尾辞）としてのみ照合する。末尾でないノードは対象でも解決しない。
        let nodes = vec![text("前の文"), text("重要"), text("後の文")];
        // 末尾 "後の文" は "重要" で終わらないので照合失敗。
        assert!(search_front_reference(&nodes, nodes.len() - 1, "重要").is_none());
        // 末尾が対象そのものなら分割（prefix 空）で解決。
        let m = search_front_reference(&nodes[..2], 1, "重要").unwrap();
        assert_eq!(m.start_idx, 1);
        assert_eq!(m.prefix, "");
        assert!(matches!(&m.children[..], [Node { kind: NodeKind::Text(s), .. }] if s == "重要"));
    }

    #[test]
    fn test_search_front_reference_suffix_split() {
        // 途中一致（これは[重要]なことだ）は解決しない＝注記のまま。
        assert!(search_front_reference(&[text("これは重要なことだ")], 0, "重要").is_none());

        // 末尾一致は分割して前半を prefix に残す。
        let nodes = [text("これは重要")];
        let m = search_front_reference(&nodes, 0, "重要").unwrap();
        assert_eq!(m.start_idx, 0);
        assert_eq!(m.prefix, "これは");
        assert!(matches!(&m.children[..], [Node { kind: NodeKind::Text(s), .. }] if s == "重要"));
    }

    #[test]
    fn test_search_front_reference_spans_reference_mentioned() {
        // 対象が「Text＋既解決の装飾ノード」にまたがるとき、装飾ノードを丸ごと
        // 子に取り込んでスパン解決する（例:「Ｘ１」で Ｘ＝Text、１＝下付き）。
        let subscript = node(NodeKind::Style {
            children: vec![text("１")],
            style_type: StyleType::Subscript,
        });
        let nodes = vec![text("前Ｘ"), subscript];
        let m = search_front_reference(&nodes, 1, "Ｘ１").unwrap();
        assert_eq!(m.start_idx, 0);
        assert_eq!(m.prefix, "前");
        // 子は古い順: [Text("Ｘ"), Subscript(１)]
        assert_eq!(m.children.len(), 2);
        assert!(matches!(&m.children[0].kind, NodeKind::Text(s) if s == "Ｘ"));
        assert!(matches!(&m.children[1].kind, NodeKind::Style { .. }));
    }
}
