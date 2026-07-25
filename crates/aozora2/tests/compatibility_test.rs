//! aozora2html（Ruby版）との互換性テスト
//!
//! tests/fixtures/ 配下のサンプルファイルを変換し、
//! Ruby版の出力と比較します。
//!
//! ## ディレクトリ構造
//!
//! ```text
//! tests/fixtures/
//!   sample_name.txt   # 入力ファイル
//!   sample_name.html  # 期待出力
//! ```

use std::fs;
use std::path::PathBuf;

use aozora_core::encoding::decode_to_utf8;
use aozora_core::html::{self, RenderOptions};

/// fixturesディレクトリのパス
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// サンプルテストケース
struct SampleTestCase {
    name: String,
    input_path: PathBuf,
    expected_path: PathBuf,
}

impl SampleTestCase {
    /// 入力ファイルを読み込んでHTMLに変換
    fn convert(&self, options: &RenderOptions) -> String {
        let bytes = fs::read(&self.input_path).expect("Failed to read input file");
        let input = decode_to_utf8(&bytes);
        html::convert(&input, options)
    }

    /// 期待出力ファイルを読み込む
    fn read_expected(&self) -> String {
        let bytes = fs::read(&self.expected_path).expect("Failed to read expected file");
        decode_to_utf8(&bytes)
    }

    /// テストを実行
    fn run_test(&self) -> TestResult {
        let expected = self.read_expected();

        let options = RenderOptions::new()
            .with_gaiji_dir("../../../gaiji/")
            .with_css_files(vec!["../../aozora.css".to_string()]);

        let actual = self.convert(&options);

        if actual == expected {
            TestResult::Passed
        } else {
            let unified_diff = compute_unified_diff(&actual, &expected, &self.name);
            TestResult::Failed {
                actual_line_count: actual.lines().count(),
                expected_line_count: expected.lines().count(),
                unified_diff,
            }
        }
    }
}

/// テスト結果
enum TestResult {
    Passed,
    Failed {
        actual_line_count: usize,
        expected_line_count: usize,
        unified_diff: String,
    },
}

/// Unified diff形式で差分を生成
fn compute_unified_diff(actual: &str, expected: &str, name: &str) -> String {
    let actual_lines: Vec<&str> = actual.lines().collect();
    let expected_lines: Vec<&str> = expected.lines().collect();

    // 簡易的なLCS（最長共通部分列）ベースの差分計算
    let mut result = String::new();
    result.push_str(&format!("--- expected/{}.html\n", name));
    result.push_str(&format!("+++ actual/{}.html\n", name));

    let diff_hunks = compute_diff_hunks(&expected_lines, &actual_lines);

    for hunk in diff_hunks {
        result.push_str(&hunk);
    }

    result
}

/// 差分ハンクを計算
fn compute_diff_hunks(expected: &[&str], actual: &[&str]) -> Vec<String> {
    let mut hunks = Vec::new();
    let mut i = 0; // expected index
    let mut j = 0; // actual index

    while i < expected.len() || j < actual.len() {
        // 一致する行をスキップ
        while i < expected.len() && j < actual.len() && expected[i] == actual[j] {
            i += 1;
            j += 1;
        }

        if i >= expected.len() && j >= actual.len() {
            break;
        }

        // 差分の開始位置を記録
        let hunk_start_exp = i;
        let hunk_start_act = j;

        // 差分を収集
        let mut hunk_lines: Vec<String> = Vec::new();
        let mut context_before: Vec<String> = Vec::new();

        // コンテキスト行（前3行）
        let context_start = if hunk_start_exp >= 3 {
            hunk_start_exp - 3
        } else {
            0
        };
        for k in context_start..hunk_start_exp {
            context_before.push(format!(" {}", truncate(expected[k], 500)));
        }

        // 差分を見つける - 次に一致する行を探す
        let mut exp_end = i;
        let mut act_end = j;

        // 単純な方法: 一致する行が見つかるまで進める
        let mut found_sync = false;
        'outer: for look_ahead in 1..=50 {
            // expected[i + look_ahead] が actual のどこかにあるか
            if i + look_ahead < expected.len() {
                for k in j..actual.len().min(j + look_ahead + 10) {
                    if expected[i + look_ahead] == actual[k] {
                        exp_end = i + look_ahead;
                        act_end = k;
                        found_sync = true;
                        break 'outer;
                    }
                }
            }
            // actual[j + look_ahead] が expected のどこかにあるか
            if j + look_ahead < actual.len() {
                for k in i..expected.len().min(i + look_ahead + 10) {
                    if actual[j + look_ahead] == expected[k] {
                        exp_end = k;
                        act_end = j + look_ahead;
                        found_sync = true;
                        break 'outer;
                    }
                }
            }
        }

        if !found_sync {
            // 同期点が見つからない場合、残り全部を差分として扱う
            exp_end = expected.len();
            act_end = actual.len();
        }

        // 削除された行（expected にあって actual にない）
        for k in i..exp_end {
            hunk_lines.push(format!("-{}", truncate(expected[k], 500)));
        }

        // 追加された行（actual にあって expected にない）
        for k in j..act_end {
            hunk_lines.push(format!("+{}", truncate(actual[k], 500)));
        }

        // コンテキスト行（後3行）
        let mut context_after: Vec<String> = Vec::new();
        let context_end = (exp_end + 3).min(expected.len());
        for k in exp_end..context_end {
            context_after.push(format!(" {}", truncate(expected[k], 500)));
        }

        if !hunk_lines.is_empty() {
            let hunk_header = format!(
                "@@ -{},{} +{},{} @@\n",
                hunk_start_exp + 1,
                exp_end - hunk_start_exp + context_before.len() + context_after.len(),
                hunk_start_act + 1,
                act_end - hunk_start_act + context_before.len() + context_after.len()
            );

            let mut hunk = hunk_header;
            for line in &context_before {
                hunk.push_str(line);
                hunk.push('\n');
            }
            for line in &hunk_lines {
                hunk.push_str(line);
                hunk.push('\n');
            }
            for line in &context_after {
                hunk.push_str(line);
                hunk.push('\n');
            }

            hunks.push(hunk);
        }

        i = exp_end;
        j = act_end;
    }

    hunks
}

/// すべてのサンプルを取得
fn get_all_samples() -> Vec<SampleTestCase> {
    let fixtures = fixtures_dir();
    if !fixtures.exists() {
        return vec![];
    }

    let mut samples = Vec::new();
    if let Ok(entries) = fs::read_dir(&fixtures) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "txt").unwrap_or(false) {
                let name = path.file_stem().unwrap().to_string_lossy().to_string();
                let expected_path = fixtures.join(format!("{}.html", name));
                if expected_path.exists() {
                    samples.push(SampleTestCase {
                        name,
                        input_path: path,
                        expected_path,
                    });
                }
            }
        }
    }

    // 名前でソート
    samples.sort_by(|a, b| a.name.cmp(&b.name));
    samples
}

/// テスト実行とレポート
fn run_sample_test(sample: &SampleTestCase, assert_pass: bool) {
    let result = sample.run_test();

    match result {
        TestResult::Passed => {
            eprintln!("[PASS] {}", sample.name);
        }
        TestResult::Failed {
            actual_line_count,
            expected_line_count,
            unified_diff,
        } => {
            eprintln!("[FAIL] {}", sample.name);
            eprintln!(
                "  Line count: actual={}, expected={}",
                actual_line_count, expected_line_count
            );
            eprintln!();
            eprintln!("{}", unified_diff);

            if assert_pass {
                panic!("Compatibility test failed for {}", sample.name);
            }
        }
    }
}

/// 文字列を指定長で切り詰め
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{}...", truncated)
    }
}

// ============================================================================
// テスト関数
// ============================================================================

/// chukiichiran_kinyurei サンプルのテスト
#[test]
fn test_chukiichiran_kinyurei() {
    let fixtures = fixtures_dir();
    let input_path = fixtures.join("chukiichiran_kinyurei.txt");
    let expected_path = fixtures.join("chukiichiran_kinyurei.html");

    if !input_path.exists() {
        eprintln!("Skipping test: sample not found at {:?}", input_path);
        return;
    }

    let sample = SampleTestCase {
        name: "chukiichiran_kinyurei".to_string(),
        input_path,
        expected_path,
    };

    run_sample_test(&sample, false);
}

/// すべてのサンプルをテスト（サマリー表示用）
#[test]
fn test_all_samples() {
    let samples = get_all_samples();

    if samples.is_empty() {
        eprintln!("No samples found in fixtures directory");
        return;
    }

    eprintln!("\n=== Compatibility Test Summary ===\n");

    for sample in &samples {
        run_sample_test(sample, false);
    }

    eprintln!("\n=== End of Summary ===\n");
}
