// 入力文字の「どの文字集合に属するか」による色分け。
//
// 青空文庫形式は JIS X 0208 を前提に書き、その外は外字注記 ※［＃…］ で書く決まりだが、
// 「外」にも段階がある。半角カナは全角に直すだけ、第3・第4水準なら外字注記に面区点を
// 書ける、JIS に無い字は説明的な注記が要る——書き手の対処が違うので色を分ける。
//
// 判定は Rust（aozora_core::charset）にあり、ここには複製しない。BMP 全体を畳んだ
// 16KB の表を起動時に一度だけ受け取り、以降はそれを引く。解析（analyze）と違って
// 1 文字ごとに完結する性質なので、IPC もデバウンスも挟まず打鍵に即追随できる。
//
// コストは表示範囲ぶんだけ。装飾はビューポート内の行だけ作り、判定はビット 1 個の
// 取り出しなので、文書がいくら大きくても一定に収まる。

import { EditorView, Decoration, ViewPlugin } from '@codemirror/view'
import type { DecorationSet, ViewUpdate } from '@codemirror/view'
import { StateField, StateEffect, RangeSetBuilder } from '@codemirror/state'
import type { Extension } from '@codemirror/state'
import {
  charsetTable,
  MARK_OTHER,
  MARK_PLAIN,
  MARK_X0201,
  MARK_X0213,
  type CharsetTable,
} from '@/commands/tauri'
import { t, onLangChange, type TranslationKey } from '@/i18n'

/** 表を引くための形に畳んだもの。 */
export interface CharsetLookup {
  /** 符号位置の区分（MARK_*）。 */
  markAt(cp: number): number
  /** 2 コードポイントで 1 文字になる組か。 */
  isComposed(pair: string): boolean
}

/** Rust から受け取った表を引ける形にする。 */
export function createLookup(table: CharsetTable): CharsetLookup {
  const bmp = Uint8Array.from(table.bmp)
  const astral = new Set(table.astral)
  const composed = new Set(table.composed)
  return {
    markAt(cp) {
      if (cp < 0x80) return MARK_PLAIN
      if (cp > 0xffff) return astral.has(cp) ? MARK_X0213 : MARK_OTHER
      return (bmp[cp >> 2] >> ((cp & 3) * 2)) & 3
    },
    isComposed: (pair) => composed.has(pair),
  }
}

/** 色を付ける範囲（オフセットは走査した文字列内の UTF-16 コード単位）。 */
export interface CharsetRun {
  from: number
  to: number
  mark: number
}

/**
 * 文字列を走査して、色を付ける範囲を隣り合う同区分ごとにまとめて返す。
 *
 * MARK_PLAIN（ASCII と JIS X 0208）は色を付けないので返さない。普通の本文は
 * すべてこれなので、たいていは空になる。
 */
export function scanText(text: string, lookup: CharsetLookup): CharsetRun[] {
  const runs: CharsetRun[] = []
  let i = 0
  while (i < text.length) {
    const cp = text.codePointAt(i) as number
    let len = cp > 0xffff ? 2 : 1
    let mark = lookup.markAt(cp)
    // 仮名＋結合半濁点（か゚ = 1-4-87 など）は 2 コードポイントで 1 文字。
    // 1 文字ずつ見ると「か＝無色 ／ ゚＝JIS外」とちぐはぐになるので組で扱う。
    if (lookup.isComposed(text.slice(i, i + len + 1))) {
      len += 1
      mark = MARK_X0213
    }
    if (mark !== MARK_PLAIN) {
      const last = runs[runs.length - 1]
      if (last && last.to === i && last.mark === mark) last.to = i + len
      else runs.push({ from: i, to: i + len, mark })
    }
    i += len
  }
  return runs
}

// --- 表の保持 --------------------------------------------------------------------
const setLookup = StateEffect.define<CharsetLookup>()

const lookupField = StateField.define<CharsetLookup | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setLookup)) return e.value
    return value
  },
})

// 表は起動時に一度取るだけ。複数のエディタが作られても取得は 1 回で済ませる。
let pending: Promise<CharsetLookup | null> | null = null

function loadLookup(): Promise<CharsetLookup | null> {
  if (!pending) {
    pending = charsetTable()
      .then(createLookup)
      // Tauri 非接続（素の vite やテスト）では取れない。色分け無しで動作継続。
      .catch(() => null)
  }
  return pending
}

// --- 装飾 -------------------------------------------------------------------------
const MARK_CLASS: Record<number, string> = {
  [MARK_X0201]: 'cm-aoz-cs-x0201',
  [MARK_X0213]: 'cm-aoz-cs-x0213',
  [MARK_OTHER]: 'cm-aoz-cs-other',
}

const MARK_TITLE: Record<number, TranslationKey> = {
  [MARK_X0201]: 'charset.x0201',
  [MARK_X0213]: 'charset.x0213',
  [MARK_OTHER]: 'charset.other',
}

// 装飾は区分ごとに 1 つ作って使い回す（同じものを毎回作らない）。title は言語に
// よって変わるので、切り替わったら作り直す。
let decorations: Record<number, Decoration> | null = null
onLangChange(() => {
  decorations = null
})

function decorationFor(mark: number): Decoration {
  if (!decorations) {
    decorations = {}
    for (const m of [MARK_X0201, MARK_X0213, MARK_OTHER]) {
      decorations[m] = Decoration.mark({
        class: MARK_CLASS[m],
        attributes: { title: t(MARK_TITLE[m]) },
      })
    }
  }
  return decorations[mark]
}

/** ビューポート内の行だけ走査して装飾を作る。 */
function build(view: EditorView): DecorationSet {
  const lookup = view.state.field(lookupField, false)
  if (!lookup) return Decoration.none

  const builder = new RangeSetBuilder<Decoration>()
  const doc = view.state.doc
  const last = doc.lineAt(view.viewport.to).number
  for (let n = doc.lineAt(view.viewport.from).number; n <= last; n++) {
    const line = doc.line(n)
    for (const run of scanText(line.text, lookup)) {
      builder.add(line.from + run.from, line.from + run.to, decorationFor(run.mark))
    }
  }
  return builder.finish()
}

const charsetPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet
    constructor(view: EditorView) {
      this.decorations = build(view)
      loadLookup().then((lookup) => {
        if (lookup) view.dispatch({ effects: setLookup.of(lookup) })
      })
    }
    update(u: ViewUpdate) {
      // 編集・スクロール・表の到着のいずれでも作り直す。表示範囲ぶんしか見ないので
      // 打鍵ごとに走らせても軽い（解析と違ってデバウンスが要らない）。
      if (
        u.docChanged ||
        u.viewportChanged ||
        u.state.field(lookupField, false) !== u.startState.field(lookupField, false)
      ) {
        this.decorations = build(u.view)
      }
    }
  },
  { decorations: (v) => v.decorations }
)

// 前景色は記法のハイライト（editor/lsp.ts）が使っているので、こちらは背景で示す。
// 同じ文字に両方かかっても打ち消し合わない。
const charsetTheme = EditorView.theme({
  '.cm-aoz-cs-x0201': { backgroundColor: 'rgba(226, 155, 30, 0.20)', borderRadius: '2px' },
  '.cm-aoz-cs-x0213': { backgroundColor: 'rgba(58, 110, 165, 0.18)', borderRadius: '2px' },
  '.cm-aoz-cs-other': { backgroundColor: 'rgba(200, 60, 60, 0.20)', borderRadius: '2px' },
})

/** 文字集合による色分け一式。 */
export const charsetHighlight: Extension = [lookupField, charsetPlugin, charsetTheme]
