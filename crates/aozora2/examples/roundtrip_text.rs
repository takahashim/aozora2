//! text → RawAST → text が全書庫で成り立つかを測る使い捨ての計測。
//! 引数は ZIP のパス列（標準入力から 1 行 1 パスでも可）。
use aozora_core::interchange::RawDocument;
use std::io::BufRead;

fn main() {
    let mut paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        paths = std::io::stdin().lock().lines().map_while(Result::ok).collect();
    }
    let (mut ok, mut ng, mut err) = (0usize, 0usize, 0usize);
    for p in &paths {
        let Ok(bytes) = aozora_core::zip::read_first_txt_from_zip(std::path::Path::new(p)) else {
            err += 1;
            continue;
        };
        let text = aozora_core::encoding::decode_to_utf8(&bytes);
        if RawDocument::from_text(&text).to_text() == text {
            ok += 1;
        } else {
            ng += 1;
            if ng <= 5 {
                eprintln!("不一致: {p}");
            }
        }
    }
    println!("一致 {ok} / 不一致 {ng} / 読めず {err}（計 {}）", paths.len());
}
