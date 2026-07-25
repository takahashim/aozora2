// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use aozora_core::analysis::{analyze as analyze_document, Analysis};
use aozora_core::html::{convert, RenderOptions};
use tauri::Manager;

/// Convert Aozora Bunko format text to HTML
#[tauri::command]
fn convert_to_html(input: &str) -> Result<String, String> {
    let options = RenderOptions::default();
    let html = convert(input, &options);
    Ok(html)
}

/// Convert file content to HTML
#[tauri::command]
fn convert_file_to_html(content: &str, filename: &str) -> Result<ConvertResult, String> {
    let html = convert_to_html(content)?;

    Ok(ConvertResult {
        html,
        filename: filename.to_string(),
    })
}

#[derive(serde::Serialize)]
struct ConvertResult {
    html: String,
    filename: String,
}

/// エディタ用の静的解析（セマンティックトークン／アウトライン／診断）を返す。
/// 位置は 0 起点（行・char）。フロント側で CodeMirror の位置に変換する。
#[tauri::command]
fn analyze(input: &str) -> Result<Analysis, String> {
    Ok(analyze_document(input))
}

/// Get the path to bundled resources
#[tauri::command]
fn get_resource_paths(app_handle: tauri::AppHandle) -> Result<ResourcePaths, String> {
    let resource_dir = app_handle
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?;

    Ok(ResourcePaths {
        gaiji: resource_dir.join("gaiji").to_string_lossy().to_string(),
        css: resource_dir.join("css").to_string_lossy().to_string(),
    })
}

#[derive(serde::Serialize)]
struct ResourcePaths {
    gaiji: String,
    css: String,
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            convert_to_html,
            convert_file_to_html,
            analyze,
            get_resource_paths,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
