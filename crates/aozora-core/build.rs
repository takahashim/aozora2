use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // jis2ucs: 値は HTML 実体参照（&#xXXXX;）または直接文字。デコードして \u{XXXX}
    // へエスケープする。デコードできない値があればビルドを失敗させる（黙って落とすと
    // 外字が解決できなくなる原因を実行時まで隠してしまうため）。
    generate_string_map(&out_dir, "jis2ucs_table.rs", "data/jis2ucs.json", |s| {
        parse_html_entities(s)
            .map(|decoded| {
                decoded
                    .chars()
                    .map(|c| format!("\\u{{{:04X}}}", c as u32))
                    .collect()
            })
            .ok_or_else(|| "HTML 実体参照（&#xXXXX;）をデコードできません".to_string())
    });

    // accent: 「基底文字＋記号」→ JISコード。値はそのまま。
    generate_string_map(&out_dir, "accent_table.rs", "data/accent_table.json", |s| {
        Ok(s.to_string())
    });

    // accent の説明文: 参照実装 aozora2html の yml/accent_table.yml 由来（規則から
    // 組み立てられない表記＝ドイツ語エスツェット等を含む）。値はそのまま。
    generate_string_map(
        &out_dir,
        "accent_name_table.rs",
        "data/accent_names.json",
        |s| Ok(s.to_string()),
    );
}

/// JSON（文字列→文字列のオブジェクト）から
/// `{ let mut m = HashMap::new(); m.insert(k, v); … m }` 形式の Rust 片を生成し、
/// `OUT_DIR/out_file` へ書き出す。各値は `transform` を通す。`transform` が `Err` を
/// 返した場合はキー付きで **panic してビルドを失敗**させる（データ不正を黙って
/// 落とさない）。生成片は `accent.rs` / `jis_table.rs` が `include!` で取り込む。
fn generate_string_map<F>(out_dir: &str, out_file: &str, in_file: &str, transform: F)
where
    F: Fn(&str) -> Result<String, String>,
{
    let dest_path = Path::new(out_dir).join(out_file);
    let json = fs::read_to_string(in_file).unwrap_or_else(|_| panic!("{in_file} not found"));
    let table: serde_json::Value = serde_json::from_str(&json).unwrap();

    let mut code = String::from("{\n    let mut m = std::collections::HashMap::new();\n");
    if let serde_json::Value::Object(map) = table {
        for (key, value) in map {
            if let serde_json::Value::String(s) = value {
                match transform(&s) {
                    Ok(v) => code.push_str(&format!("    m.insert(\"{key}\", \"{v}\");\n")),
                    Err(e) => {
                        panic!("{in_file}: キー {key:?} の値 {s:?} を変換できません: {e}")
                    }
                }
            }
        }
    }
    code.push_str("    m\n}");
    fs::write(&dest_path, code).unwrap();
    println!("cargo:rerun-if-changed={in_file}");
}

fn parse_html_entities(s: &str) -> Option<String> {
    let mut result = String::new();
    let mut remaining = s;

    while !remaining.is_empty() {
        if remaining.starts_with("&#x") {
            // &#xXXXX; 形式
            if let Some(end) = remaining.find(';') {
                let hex = &remaining[3..end];
                if let Ok(code) = u32::from_str_radix(hex, 16) {
                    if let Some(ch) = char::from_u32(code) {
                        result.push(ch);
                        remaining = &remaining[end + 1..];
                        continue;
                    }
                }
            }
            return None; // パース失敗
        } else {
            // 直接Unicode文字
            if let Some(ch) = remaining.chars().next() {
                result.push(ch);
                remaining = &remaining[ch.len_utf8()..];
            } else {
                break;
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}
