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

    // JIS X 0208 の漢字 6,355 字のビットマップ。
    generate_x0208_kanji_bitmap(&out_dir);
}

/// URO（U+4E00-U+9FFF）の各符号位置が **JIS X 0208 の漢字**かを表すビットマップを
/// `OUT_DIR/x0208_kanji_bitmap.rs` へ書き出す（`[u8; 2624]` の配列リテラル）。
///
/// 出典は `data/jis2ucs.json`（JIS X 0213 の面区点 → Unicode）。JIS X 0208 の漢字は
/// X 0213 面1 のうち **16-01〜47-51（第1水準 2,965字）** と
/// **48-01〜84-06（第2水準 3,390字）** で、計 **6,355字**。
/// 区47 点52〜94 と 区84 点7〜94 は X 0213 で追加された 131 字なので除外する。
///
/// 漢字判定を Unicode レンジで書くと 4,031 本に散らばるが、区点空間では上記2レンジで
/// 済む。生成後は実行時に O(1) のビット参照で判定でき、エンコーディング実装に依存しない。
fn generate_x0208_kanji_bitmap(out_dir: &str) {
    const URO_START: u32 = 0x4E00;
    const URO_END: u32 = 0x9FFF;
    const LEN: usize = ((URO_END - URO_START + 1) / 8) as usize; // 2624

    let in_file = "data/jis2ucs.json";
    let json = fs::read_to_string(in_file).unwrap_or_else(|_| panic!("{in_file} not found"));
    let table: serde_json::Value = serde_json::from_str(&json).unwrap();

    let mut bits = vec![0u8; LEN];
    let mut count = 0usize;
    if let serde_json::Value::Object(map) = table {
        for (key, value) in map {
            let Some((men, ku, ten)) = parse_menkuten(&key) else {
                continue;
            };
            if !is_x0208_kanji_cell(men, ku, ten) {
                continue;
            }
            let serde_json::Value::String(s) = value else {
                continue;
            };
            let decoded = parse_html_entities(&s)
                .unwrap_or_else(|| panic!("{in_file}: {key} の値 {s:?} をデコードできません"));
            let mut chars = decoded.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                panic!("{in_file}: {key} が単一文字ではありません: {decoded:?}");
            };
            let cp = c as u32;
            assert!(
                (URO_START..=URO_END).contains(&cp),
                "{in_file}: {key} ({c}, U+{cp:04X}) が URO の外にあります"
            );
            let idx = (cp - URO_START) as usize;
            bits[idx / 8] |= 1 << (idx % 8);
            count += 1;
        }
    }
    assert_eq!(
        count, 6355,
        "JIS X 0208 の漢字数が 6,355 になりません（区点レンジの抽出条件を確認）"
    );

    let mut code = String::from("[\n");
    for chunk in bits.chunks(16) {
        code.push_str("    ");
        for b in chunk {
            code.push_str(&format!("{b:#04x}, "));
        }
        code.push('\n');
    }
    code.push(']');
    fs::write(Path::new(out_dir).join("x0208_kanji_bitmap.rs"), code).unwrap();
    println!("cargo:rerun-if-changed={in_file}");
}

/// `"1-16-01"` 形式のキーを (面, 区, 点) に分解する。形式が違えば `None`。
fn parse_menkuten(key: &str) -> Option<(u32, u32, u32)> {
    let mut it = key.split('-');
    let men = it.next()?.parse().ok()?;
    let ku = it.next()?.parse().ok()?;
    let ten = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((men, ku, ten))
}

/// その面区点が JIS X 0208 の漢字か（面1 の 16-01〜47-51 ＋ 48-01〜84-06）。
fn is_x0208_kanji_cell(men: u32, ku: u32, ten: u32) -> bool {
    if men != 1 {
        return false;
    }
    match ku {
        16..=46 => true,
        47 => ten <= 51, // 第1水準の末尾。47-52 以降は X 0213 の追加分
        48..=83 => true,
        84 => ten <= 6, // 第2水準の末尾（84-05 凜, 84-06 熙 は X 0208-1990 で追加）
        _ => false,
    }
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
