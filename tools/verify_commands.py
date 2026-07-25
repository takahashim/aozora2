#!/usr/bin/env python3
"""青空文庫記法コマンドの網羅・一致検証ツール。

参照実装 (aozora2html, Ruby) が認識する全コマンドを列挙し、aozora2 (Rust) の
出力と突き合わせて「対応漏れ」「誤実装」を検出する。

2段階で検証する:

  1. 静的カバレッジ: 参照の *_COMMAND 定数・INDENT_TYPE・command_table.yml から
     全コマンド名を抽出し、Rust ソースに文字列として現れるか確認する。
     → コマンドが「丸ごと欠けている」ケースを検出する。ただし「存在するが
       誤配線」（例: 割書→warichu）は検出できない点に注意。

  2. 挙動比較（本命）: 各コマンドの最小フィクスチャを参照 Ruby と aozora2 の
     両方に通し、本文(main_text)の出力を正規化して差分する。
     → 誤実装を確実に検出する。

使い方:
    python3 tools/verify_commands.py \
        --ruby-dir /path/to/aozora2html \
        --a2 /path/to/aozora2/target/release/aozora2

いずれも省略時は本リポジトリ隣の ../aozora2html と target/release/aozora2 を使う。
"""
from __future__ import annotations
import argparse
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_RUBY = os.path.normpath(os.path.join(HERE, "..", "..", "aozora2html"))
DEFAULT_A2 = os.path.normpath(os.path.join(HERE, "..", "target", "release", "aozora2"))

# --- 各コマンドの最小フィクスチャ（本文に差し込む1〜複数行） ---
# 参照 Ruby と aozora2 の両方に同じ入力を与えて main_text の出力を比較する。
FIXTURES = {
    # インライン装飾（後方参照 「対象」に/は<装飾>）
    "傍点": "対象［＃「対象」に傍点］",
    "白ゴマ傍点": "対象［＃「対象」に白ゴマ傍点］",
    "丸傍点": "対象［＃「対象」に丸傍点］",
    "白丸傍点": "対象［＃「対象」に白丸傍点］",
    "黒三角傍点": "対象［＃「対象」に黒三角傍点］",
    "白三角傍点": "対象［＃「対象」に白三角傍点］",
    "二重丸傍点": "対象［＃「対象」に二重丸傍点］",
    "蛇の目傍点": "対象［＃「対象」に蛇の目傍点］",
    "ばつ傍点": "対象［＃「対象」にばつ傍点］",
    "傍線": "対象［＃「対象」に傍線］",
    "二重傍線": "対象［＃「対象」に二重傍線］",
    "鎖線": "対象［＃「対象」に鎖線］",
    "破線": "対象［＃「対象」に破線］",
    "波線": "対象［＃「対象」に波線］",
    "太字": "対象［＃「対象」は太字］",
    "斜体": "対象［＃「対象」は斜体］",
    "下付き小文字": "対象［＃「対象」は下付き小文字］",
    "上付き小文字": "対象［＃「対象」は上付き小文字］",
    "行右小書き": "対象［＃「対象」は行右小書き］",
    "行左小書き": "対象［＃「対象」は行左小書き］",
    # ブロック/インラインスタイル
    "割書": "［＃割書］夏期演説［＃割書終わり］",
    "横組み": "［＃横組み］12［＃横組み終わり］",
    "キャプション": "［＃キャプション］図［＃キャプション終わり］",
    "罫囲み": "［＃罫囲み］囲［＃罫囲み終わり］",
    "割り注": "本文［＃割り注］注記［＃割り注終わり］",
    # 見出し（大中小 × 通常/同行/窓）
    "大見出し": "「章」は大見出し\r\n章題",
    "中見出し": "「節」は中見出し\r\n節題",
    "小見出し": "「項」は小見出し\r\n項題",
    "同行大見出し": "「甲」は同行大見出し",
    "同行中見出し": "「乙」は同行中見出し",
    "同行小見出し": "「丙」は同行小見出し",
    "窓大見出し": "「ａ」は窓大見出し",
    "窓中見出し": "「ｂ」は窓中見出し",
    "窓小見出し": "「ｃ」は窓小見出し",
    # ブロック字下げ系
    "字下げ": "［＃ここから２字下げ］\r\n内容\r\n［＃ここで字下げ終わり］",
    "地付き": "［＃地付き］末尾",
    "字詰め": "［＃ここから10字詰め］\r\n内容\r\n［＃ここで字詰め終わり］",
    "字上げ": "［＃ここから２字上げ］\r\n内容\r\n［＃ここで字上げ終わり］",
    "折り返して": "［＃ここから１字下げ、折り返して３字下げ］\r\n内容\r\n［＃ここで字下げ終わり］",
    "この行": "［＃この行２字下げ］行内容",
    "天付き": "［＃天付き、折り返して２字下げ］\r\n内容\r\n［＃ここで字下げ終わり］",
    "大きな文字": "［＃ここから２段階大きな文字］\r\n大\r\n［＃ここで大きな文字終わり］",
    "小さな文字": "［＃ここから２段階小さな文字］\r\n小\r\n［＃ここで小さな文字終わり］",
    # その他インライン
    "縦中横": "12［＃「12」は縦中横］",
    "返り点": "學而時習之［＃「而」の左に返り点レ］",
    "訓点送り仮名": "學［＃「學」の右に訓点送り仮名ブ］",
    "注記付き": "呼吸［＃「呼吸」の注記付き］",
    "写真": "キャプション（fig001_01.png、横100×縦200）入る",
}


def build_doc(body: str) -> bytes:
    """最小の妥当な青空文庫ファイル（CRLF・SJIS）を組み立てる。"""
    txt = f"テスト\r\nテスト\r\n\r\n{body}\r\n底本：テスト\r\n"
    return txt.encode("cp932", "replace")


def main_text(html: str) -> str:
    """main_text セクションの中身だけを取り出し、CR を除いて正規化する。"""
    m = re.search(r'main_text">(.*?)</div>\s*<div class="bibliographical', html, re.S)
    s = m.group(1).strip() if m else html
    return s.replace("\r", "")


def run_ruby(ruby_dir: str, body: str) -> str:
    with tempfile.NamedTemporaryFile(suffix=".txt", delete=False) as f:
        f.write(build_doc(body))
        src = f.name
    dst = src.replace(".txt", ".html")
    subprocess.run(
        ["ruby", "-Ilib", "bin/aozora2html", src, dst],
        cwd=ruby_dir, capture_output=True,
    )
    html = ""
    if os.path.exists(dst):
        html = open(dst, encoding="cp932", errors="replace").read()
        os.unlink(dst)
    os.unlink(src)
    return html


def run_a2(a2: str, body: str) -> str:
    with tempfile.NamedTemporaryFile(suffix=".txt", delete=False) as f:
        f.write(build_doc(body))
        src = f.name
    r = subprocess.run([a2, "html", src], capture_output=True)
    os.unlink(src)
    return r.stdout.decode("cp932", errors="replace")


def static_coverage(ruby_dir: str, rust_src: str) -> list[str]:
    rb = open(os.path.join(ruby_dir, "lib", "aozora2html.rb"), encoding="utf-8").read()
    cmds = set()
    for m in re.finditer(r"[A-Z_]+_COMMAND\s*=\s*'([^']+)'", rb):
        cmds.add(m.group(1))
    mt = re.search(r"INDENT_TYPE\s*=\s*\{(.*?)\}", rb, re.S)
    if mt:
        for m in re.finditer(r":\s*'([^']+)'", mt.group(1)):
            cmds.add(m.group(1))
    yml = os.path.join(ruby_dir, "yml", "command_table.yml")
    if os.path.exists(yml):
        for line in open(yml, encoding="utf-8"):
            line = line.rstrip("\n")
            if re.match(r"^\S.*:$", line):
                cmds.add(line[:-1].strip())
    # 見出しは大/中/小 + 同行/窓 の合成で処理されるため、部分文字列で判定する。
    missing = []
    for c in sorted(cmds):
        if c in rust_src:
            continue
        # 合成コマンドの緩和判定
        if "見出し" in c and "見出" in rust_src:
            continue
        missing.append(c)
    return missing


def collect_rust_src(a2_bin: str) -> str:
    root = os.path.normpath(os.path.join(os.path.dirname(a2_bin), "..", "..", "crates"))
    src = ""
    for dp, _, fs in os.walk(root):
        for fn in fs:
            if fn.endswith(".rs"):
                src += open(os.path.join(dp, fn), encoding="utf-8", errors="replace").read()
    return src


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--ruby-dir", default=DEFAULT_RUBY)
    ap.add_argument("--a2", default=DEFAULT_A2)
    ap.add_argument("--verbose", "-v", action="store_true", help="一致したものも表示")
    args = ap.parse_args()

    if not os.path.exists(args.a2):
        print(f"aozora2 バイナリが見つかりません: {args.a2}", file=sys.stderr)
        return 2
    ruby_ok = os.path.exists(os.path.join(args.ruby_dir, "bin", "aozora2html"))

    # 1) 静的カバレッジ
    if ruby_ok:
        rust_src = collect_rust_src(args.a2)
        missing = static_coverage(args.ruby_dir, rust_src)
        print("== 静的カバレッジ（コマンド名がRustソースに存在するか）==")
        print("  全コマンド文字列を確認" if not missing else f"  ★未出現: {missing}")
        print()

    # 2) 挙動比較
    print("== 挙動比較（参照Ruby vs aozora2, main_text 差分）==")
    if not ruby_ok:
        print(f"  参照Rubyが無いためスキップ（--ruby-dir 指定）: {args.ruby_dir}")
        return 0
    diffs = []
    for cmd, body in FIXTURES.items():
        rb = main_text(run_ruby(args.ruby_dir, body))
        a2 = main_text(run_a2(args.a2, body))
        ok = rb == a2
        if not ok:
            diffs.append((cmd, rb, a2))
        if args.verbose or not ok:
            print(f"  {'✓' if ok else '✗'} {cmd}")
    print(f"\n  差分 {len(diffs)}/{len(FIXTURES)} 件")
    for cmd, rb, a2 in diffs:
        print(f"\n### {cmd}")
        print(f"  参照: {rb[:200]}")
        print(f"  我々: {a2[:200]}")
    return 1 if diffs else 0


if __name__ == "__main__":
    sys.exit(main())
