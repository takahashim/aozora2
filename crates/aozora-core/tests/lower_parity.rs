//! 既定経路 [`lower_to_blocks_with_diagnostics`] と、移行前の凍結コピー
//! [`lower_to_blocks_legacy`] の並走検証。
//!
//! 制約解消型 Lowerer への移行（`docs/plan-lowerer-migration.md`）のあいだ、行ループを
//! 段階的に作り替えても **AST と診断が 1 バイトも変わらない**ことをここで縛る。移行が
//! 終われば凍結コピーごと消える（計画の C6）。
//!
//! feature ゲートを付けない。CI の test ジョブは素の `cargo test` を回すので、
//! `#![cfg(feature = "serde")]` の下に置くと走らない。conformance のフィクスチャは
//! dev-dependency の `serde_json::Value` として読み、feature には依存しない。
//!
//! AST の比較を `format!("{:#?}")` の文字列で行うのは意図的である。`Block`/`Inline` の
//! 手書き `PartialEq` は `line`/`span` を比較しないので、`==` にすると行番号の退行を
//! 検出できない。**PartialEq 比較に退化させないこと。**

use std::path::{Path, PathBuf};

use aozora_core::lower::{lower_to_blocks_legacy, lower_to_blocks_with_diagnostics};
use aozora_core::parser::parse_document_raw;

/// 入力 1 件を両経路に通し、AST（デバッグ表示）と診断を突き合わせる。
fn assert_parity(label: &str, text: &str) {
    let lines: Vec<&str> = text.split("\r\n").collect();
    let raw = parse_document_raw(&lines);

    let (current_ast, current_diags) = lower_to_blocks_with_diagnostics(&raw);
    let (legacy_ast, legacy_diags) = lower_to_blocks_legacy(&raw);

    let current = format!("{current_ast:#?}");
    let legacy = format!("{legacy_ast:#?}");
    if current != legacy {
        panic!(
            "{label}: AST が凍結コピーと食い違う\n{}",
            first_difference(&legacy, &current)
        );
    }
    assert_eq!(
        legacy_diags, current_diags,
        "{label}: 診断が凍結コピーと食い違う"
    );
}

/// 最初に食い違った行の前後 3 行を並べて返す（依存を足さずに読める失敗表示にする）。
fn first_difference(legacy: &str, current: &str) -> String {
    let legacy: Vec<&str> = legacy.lines().collect();
    let current: Vec<&str> = current.lines().collect();
    let at = (0..legacy.len().max(current.len()))
        .find(|i| legacy.get(*i) != current.get(*i))
        .expect("食い違う行がある");
    let from = at.saturating_sub(3);
    let to = (at + 4).min(legacy.len().max(current.len()));

    let mut out = format!("最初の相違は {} 行目（1 起点）\n", at + 1);
    for (name, side) in [("凍結", &legacy), ("現行", &current)] {
        out.push_str(&format!("--- {name} ---\n"));
        for i in from..to {
            let mark = if i == at { ">" } else { " " };
            out.push_str(&format!("{mark} {}\n", side.get(i).unwrap_or(&"<行なし>")));
        }
    }
    out
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 入力 1: 交換形式の適合フィクスチャ 35 件の `source`。
///
/// `conformance.rs` と同じく LF 区切りで書かれているので CRLF に直す。serde の
/// feature に依存しないよう、`Fixture` 型ではなく `Value` の `source` だけを読む。
#[test]
fn conformance_sources_lower_identically() {
    let dir = manifest_dir().join("data/conformance");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("data/conformance が読める")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "フィクスチャが 1 件も無い");

    for path in paths {
        let text = std::fs::read_to_string(&path).expect("読める");
        let value: serde_json::Value = serde_json::from_str(&text).expect("形式が壊れている");
        let source = value["source"].as_str().expect("source がある");
        assert_parity(&path.display().to_string(), &source.replace('\n', "\r\n"));
    }
}

/// 入力 2: 実文書のフィクスチャ（注記一覧の全例・記入例と、実際の作品 1 本）。
///
/// `chukiichiran_zenrei.txt` は注記一覧の全例 1068 行で、単一入力としては最も分岐
/// カバレッジが高い。**無ければ警告して skip する**——crates.io に aozora-core だけが
/// パッケージされた状態の `cargo test` を壊さないため。
#[test]
fn document_fixtures_lower_identically() {
    let dir = manifest_dir().join("../aozora2/tests/fixtures");
    let names = [
        "chukiichiran_zenrei.txt",
        "chukiichiran_kinyurei.txt",
        "junshi.txt",
    ];
    let mut ran = 0;
    for name in names {
        let path = dir.join(name);
        if !Path::new(&path).exists() {
            eprintln!("skip: {} が無い", path.display());
            continue;
        }
        let bytes = std::fs::read(&path).expect("読める");
        let text = aozora_core::encoding::decode_to_utf8(&bytes);
        assert_parity(name, &text);
        ran += 1;
    }
    eprintln!("実文書フィクスチャ {ran}/{} 件を検証した", names.len());
}

/// 入力 3: 記法を一通り含む複合入力（`invariants.rs` の SOURCE 相当）。
const COMPOSITE: &str = "東京《とうきょう》へ\r\n\
     本文［＃「本文」に傍点］と※［＃「けものへん＋苗」、第3水準1-87-63］\r\n\
     ［＃ここから２字下げ］\r\n\
     中身［＃「中身」は中見出し］\r\n\
     ［＃ここで字下げ終わり］つづき\r\n\
     ［＃３字下げ］行スコープの包み\r\n\
     〔未閉じの行\r\n\
     次の行\r\n\
     ［＃ここから改行天付き、折り返して１字下げ］開始行\r\n\
     ぶら下げの中\r\n\
     ［＃ここで字下げ終わり］\r\n\
     解決できない［＃「存在しない対象」に傍点］\r\n\
     12［＃「12」は縦中横］［＃割り注］注［＃割り注終わり］";

#[test]
fn composite_source_lowers_identically() {
    assert_parity("composite", COMPOSITE);
}

/// 入力 4: 手書きのエッジ表。
///
/// `lower/mod.rs` の position_tests が使う入力と、移行計画で挙げた実測済みのエッジ
/// （`docs/spec-lowerer-constraints.md` の制約が食い違いやすいと名指しした形）を並べる。
/// リポジトリ内の入力は小規模なので、C1〜C5 の差はコーパスでしか出ない種類もある。
/// ここは早期検出用で、最終的な保証は全書庫並走（`crates/aozora2/examples/lower_parity.rs`）が持つ。
const EDGES: &[(&str, &str)] = &[
    ("行番号", "一行目\r\n［＃ここから２字下げ］\r\n中身\r\n［＃ここで字下げ終わり］"),
    ("閉じの後に本文", "［＃ここから２字下げ］\r\n本文［＃ここで字下げ終わり］つづき"),
    ("閉じを挟む本文", "［＃ここから太字］\r\n前［＃ここで太字終わり］後"),
    ("行途中オープン", "本文［＃ここから斜体］つづき"),
    ("行途中オープン・前がルビ", "東京《とうきょう》［＃ここから斜体］つづき"),
    ("同行開閉", "本文［＃ここから太字］中［＃ここで太字終わり］後"),
    (
        "頭にインライン記法のある閉じ行",
        "［＃ここから２字下げ］\r\n東京《とうきょう》［＃ここで字下げ終わり］",
    ),
    ("割り注終わりは閉じない", "［＃ここから２字下げ］\r\n本文［＃割り注］注［＃割り注終わり］\r\n［＃ここで字下げ終わり］"),
    ("単独行の割り注終わり", "［＃ここから２字下げ］\r\n［＃割り注終わり］\r\n［＃ここで字下げ終わり］"),
    (
        "1 行に複数の閉じ",
        "［＃ここから２字下げ］\r\n［＃ここから小さな文字］\r\n本文［＃ここで小さな文字終わり］［＃ここで字下げ終わり］",
    ),
    ("行内地付きと地付き終わり", "［＃ここから地付き］\r\n本文［＃地付き］うしろ［＃ここで地付き終わり］"),
    ("開き無しの閉じ（行途中）", "本文［＃ここで字下げ終わり］あと"),
    ("開き無しの閉じ（単独行）", "本文\r\n［＃ここで字下げ終わり］\r\nあと"),
    (
        "裸の終端と明示終端が同居",
        "［＃ここから２字下げ］\r\nＡ［＃字下げ終わり］Ｂ［＃ここで字下げ終わり］Ｃ",
    ),
    ("行スコープ字下げの重ね", "｜［＃２字下げ］あいう《るび》［＃４字下げ］えお"),
    ("字下げ中の単独字下げ行", "［＃ここから２字下げ］\r\n本文\r\n［＃４字下げ］\r\n中身\r\n［＃ここで字下げ終わり］"),
    ("行スコープ字下げの並記", "［＃２字下げ］あ［＃５字下げ］い"),
    ("幅なしの字下げ", "［＃ここから字下げ］\r\n本文\r\n［＃ここで字下げ終わり］"),
    ("行スコープ包み＋未閉じ〔＋空行", "［＃地から１字上げ］〔あいう\r\n\r\n次の行"),
    ("未閉じ〔の連続", "〔あ\r\n〔い\r\nうえお"),
    (
        "ぶら下げ開始行 3 連続",
        "［＃ここから改行天付き、折り返して１字下げ］あ\r\n［＃ここから改行天付き、折り返して２字下げ］い\r\n［＃ここから改行天付き、折り返して３字下げ］う\r\n本文",
    ),
    (
        "ぶら下げ開始行の直後にブロック開始行",
        "［＃ここから改行天付き、折り返して１字下げ］あ\r\n［＃ここから２字下げ］\r\n本文\r\n［＃ここで字下げ終わり］",
    ),
    (
        "ぶら下げの中で装飾が閉じる",
        "［＃ここから改行天付き、折り返して１字下げ］\r\n［＃ここから太字］\r\n本文\r\n［＃ここで太字終わり］\r\n［＃ここで字下げ終わり］",
    ),
    ("地付きの暗黙閉じ", "［＃ここから地付き］\r\nあ\r\n［＃ここから地付き］\r\nい\r\n［＃ここで地付き終わり］"),
    ("字下げの暗黙閉じ", "［＃ここから２字下げ］\r\nあ\r\n［＃ここから４字下げ］\r\nい\r\n［＃ここで字下げ終わり］"),
    ("EOF 多重未閉じ", "［＃ここから２字下げ］\r\n［＃ここから太字］\r\n本文"),
    ("見出しブロック", "［＃ここから中見出し］\r\n見出し\r\n［＃ここで中見出し終わり］"),
    ("縦中横の単独終わり行", "［＃ここから２字下げ］\r\n［＃縦中横終わり］\r\n本文"),
    ("空文書", ""),
    ("空行だけ", "\r\n\r\n"),
];

#[test]
fn hand_written_edges_lower_identically() {
    for (label, source) in EDGES {
        assert_parity(label, source);
    }
}
