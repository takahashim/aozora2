use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("jis2ucs_table.rs");

    // data/jis2ucs.json を読み込み
    let json = fs::read_to_string("data/jis2ucs.json").expect("data/jis2ucs.json not found");
    let table: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Rustのハッシュマップ初期化コードを生成
    let mut code = String::from("{\n    let mut m = std::collections::HashMap::new();\n");

    if let serde_json::Value::Object(map) = table {
        for (key, value) in map {
            if let serde_json::Value::String(s) = value {
                // HTML実体参照 "&#xXXXX;" または "&#xXXXX;&#xYYYY;" を文字列に変換
                if let Some(decoded) = parse_html_entities(&s) {
                    // 文字列をエスケープして出力
                    let escaped: String = decoded
                        .chars()
                        .map(|c| format!("\\u{{{:04X}}}", c as u32))
                        .collect();
                    code.push_str(&format!(
                        "    m.insert(\"{}\", \"{}\");\n",
                        key, escaped
                    ));
                }
            }
        }
    }

    code.push_str("    m\n}");

    fs::write(&dest_path, code).unwrap();

    // ファイル変更時に再ビルド
    println!("cargo:rerun-if-changed=data/jis2ucs.json");
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
