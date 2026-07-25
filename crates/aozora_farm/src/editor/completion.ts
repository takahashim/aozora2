// 青空文庫注記の補完。`［＃` を打つと直後に入る注記候補を出す。
// 候補を選ぶと ［＃…］ 一式を挿入し、必要な位置（「」の中など）へカーソルを置く。

import { autocompletion } from '@codemirror/autocomplete'
import type { CompletionContext, CompletionResult, Completion } from '@codemirror/autocomplete'
import { EditorView } from '@codemirror/view'

/** 補完候補の定義。 */
export interface Snippet {
  /** 表示ラベル（注記の語）。 */
  label: string
  /** 種別（decoration/structure/block）。detail に出す。 */
  detail: string
  /** 挿入する完全形（`［＃…］`。開閉ペアは両方）。 */
  apply: string
  /** 挿入後のカーソル位置（apply 内 char オフセット。省略時は末尾）。 */
  cursor?: number
  /** 絞り込み用の読み（ひらがな）。 */
  reading: string
}

// 「」の内側にカーソルを置くものは cursor=3（［＃「 の直後）。
// 開閉ペアは開き ［＃…］ の直後にカーソルを置く。
export const SNIPPETS: Snippet[] = [
  // 強調・装飾（対象を「」に入れる形）
  { label: '傍点', detail: '強調', apply: '［＃「」に傍点］', cursor: 3, reading: 'ぼうてん' },
  { label: '二重傍点', detail: '強調', apply: '［＃「」に二重傍点］', cursor: 3, reading: 'にじゅうぼうてん' },
  { label: '傍線', detail: '強調', apply: '［＃「」に傍線］', cursor: 3, reading: 'ぼうせん' },
  { label: '二重傍線', detail: '強調', apply: '［＃「」に二重傍線］', cursor: 3, reading: 'にじゅうぼうせん' },
  { label: '斜体', detail: '強調', apply: '［＃「」に斜体］', cursor: 3, reading: 'しゃたい' },
  // 太字は開閉ペア（開きの直後にカーソル）
  { label: '太字', detail: '強調', apply: '［＃太字］［＃太字終わり］', cursor: 5, reading: 'ふとじ' },
  // 見出し（対象を「」に）
  { label: '大見出し', detail: '構造', apply: '［＃「」は大見出し］', cursor: 3, reading: 'おおみだし' },
  { label: '中見出し', detail: '構造', apply: '［＃「」は中見出し］', cursor: 3, reading: 'なかみだし' },
  { label: '小見出し', detail: '構造', apply: '［＃「」は小見出し］', cursor: 3, reading: 'こみだし' },
  // 字下げ・地付き
  { label: '字下げ', detail: '構造', apply: '［＃２字下げ］', cursor: 2, reading: 'じさげ' },
  { label: '地付き', detail: '構造', apply: '［＃地付き］', reading: 'じつき' },
  { label: '改ページ', detail: '構造', apply: '［＃改ページ］', reading: 'かいぺーじ' },
  { label: 'ページの左右中央', detail: '構造', apply: '［＃ページの左右中央］', reading: 'ぺーじのさゆうちゅうおう' },
  // ブロック（開始／終了ペア。開始の直後にカーソル）
  {
    label: 'ここから字下げ',
    detail: 'ブロック',
    apply: '［＃ここから２字下げ］\n\n［＃ここで字下げ終わり］',
    cursor: 12, // 開始行＋改行の後（空の中間行）
    reading: 'ここからじさげ',
  },
  {
    label: 'ここから引用',
    detail: 'ブロック',
    apply: '［＃ここから引用］\n\n［＃ここで引用終わり］',
    cursor: 10, // 開始行＋改行の後（空の中間行）
    reading: 'ここからいんよう',
  },
  {
    label: 'ここから罫囲み',
    detail: 'ブロック',
    apply: '［＃ここから罫囲み］\n\n［＃ここで罫囲み終わり］',
    cursor: 11, // 開始行＋改行の後（空の中間行）
    reading: 'ここからけいがこみ',
  },
]

/** 入力語（［＃ の後ろ）で候補を絞る。空なら全件。 */
export function filterSnippets(typed: string): Snippet[] {
  const q = typed.trim()
  if (!q) return SNIPPETS
  return SNIPPETS.filter((s) => s.label.includes(q) || s.reading.includes(q))
}

/** ［＃<語> にマッチする補完ソース。 */
function source(ctx: CompletionContext): CompletionResult | null {
  // カーソル直前の ［＃ から未閉じ ］ まで。改行・］ は含めない。
  const before = ctx.matchBefore(/［＃[^］\n]*/)
  if (!before || before.from === before.to) return null

  const typed = before.text.slice(2) // ［＃ を除いた部分
  const snippets = filterSnippets(typed)
  if (snippets.length === 0) return null

  const options: Completion[] = snippets.map((s) => ({
    label: s.label,
    detail: s.detail,
    type: 'keyword',
    apply: (view: EditorView, _c: Completion, from: number, to: number) => {
      view.dispatch({
        changes: { from, to, insert: s.apply },
        selection: { anchor: from + (s.cursor ?? s.apply.length) },
      })
    },
  }))

  // from は ［ の位置。自前で絞るので filter:false。
  return { from: before.from, options, filter: false }
}

/** 注記補完の拡張。 */
export const aozoraCompletion = autocompletion({ override: [source] })
