//! 全書庫規模の並走検証。既定経路 `lower_to_blocks_with_diagnostics` と、移行前の
//! 凍結コピー `lower_to_blocks_legacy` が同じ AST・診断を返すかを測る使い捨ての計測。
//! 引数は ZIP のパス列（標準入力から 1 行 1 パスでも可）。
//!
//! ```text
//! cargo build --release --example lower_parity -p aozora2
//! find ../aozorabunko/cards -name '*.zip' | ./target/release/examples/lower_parity
//! ```
//!
//! 移行（`docs/plan-lowerer-migration.md`）が終われば凍結コピーごと消える。
use aozora_core::lower::{lower_to_blocks_legacy, lower_to_blocks_with_diagnostics};
use aozora_core::parser::parse_document_raw;
use std::io::BufRead;

fn main() {
    let mut paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        paths = std::io::stdin()
            .lock()
            .lines()
            .map_while(Result::ok)
            .collect();
    }
    let (mut ok, mut ng, mut err) = (0usize, 0usize, 0usize);
    for p in &paths {
        let Ok(bytes) = aozora_core::zip::read_first_txt_from_zip(std::path::Path::new(p)) else {
            err += 1;
            continue;
        };
        let text = aozora_core::encoding::decode_to_utf8(&bytes);
        let lines: Vec<&str> = text.split("\r\n").collect();
        let raw = parse_document_raw(&lines);

        let (current_ast, current_diags) = lower_to_blocks_with_diagnostics(&raw);
        let (legacy_ast, legacy_diags) = lower_to_blocks_legacy(&raw);

        // AST は `{:#?}` で比較する。手書き `PartialEq` は line/span を見ないので、
        // `==` にすると行番号の退行を見逃す（tests/lower_parity.rs と同じ理由）。
        let current = format!("{current_ast:#?}");
        let legacy = format!("{legacy_ast:#?}");
        if current == legacy && current_diags == legacy_diags {
            ok += 1;
            continue;
        }
        ng += 1;
        if ng <= 5 {
            if current != legacy {
                eprintln!("不一致(AST): {p}\n{}", first_difference(&legacy, &current));
            } else {
                eprintln!("不一致(診断): {p}\n  凍結 {legacy_diags:?}\n  現行 {current_diags:?}");
            }
        }
    }
    println!(
        "一致 {ok} / 不一致 {ng} / 読めず {err}（計 {}）",
        paths.len()
    );
    if ng > 0 {
        std::process::exit(1);
    }
}

/// 最初に食い違った行の前後 3 行。
fn first_difference(legacy: &str, current: &str) -> String {
    let legacy: Vec<&str> = legacy.lines().collect();
    let current: Vec<&str> = current.lines().collect();
    let Some(at) = (0..legacy.len().max(current.len())).find(|i| legacy.get(*i) != current.get(*i))
    else {
        return String::new();
    };
    let from = at.saturating_sub(3);
    let to = (at + 4).min(legacy.len().max(current.len()));
    let mut out = format!("  最初の相違は {} 行目（1 起点）\n", at + 1);
    for (name, side) in [("凍結", &legacy), ("現行", &current)] {
        out.push_str(&format!("  --- {name} ---\n"));
        for i in from..to {
            let mark = if i == at { ">" } else { " " };
            out.push_str(&format!(
                "  {mark} {}\n",
                side.get(i).unwrap_or(&"<行なし>")
            ));
        }
    }
    out
}
