// 青空文庫記法の LSP 的支援（フェーズ1: 診断＋セマンティックハイライト）。
//
// Rust の aozora_core::analysis を Tauri 経由で呼び（commands/tauri.ts の analyze）、
// 返る Analysis（0 起点の行・char 範囲）を CodeMirror の位置に変換して、
// - 診断（linter の下線＋ガター）
// - セマンティックハイライト（Decoration.mark）
// に反映する。解析はドキュメント変更をデバウンスして 1 回だけ呼ぶ。
//
// 位置は Rust 側が LSP 準拠の 0 起点、CodeMirror は行 1 起点なので toPos で変換する。

import { EditorView, Decoration, ViewPlugin, hoverTooltip, showPanel } from '@codemirror/view'
import type { DecorationSet, ViewUpdate, Tooltip, Panel } from '@codemirror/view'
import { StateField, StateEffect, RangeSetBuilder } from '@codemirror/state'
import type { Extension, Text } from '@codemirror/state'
import { setDiagnostics, lintGutter } from '@codemirror/lint'
import type { Diagnostic as CmDiagnostic } from '@codemirror/lint'
import { codeFolding, foldGutter, foldService } from '@codemirror/language'
import {
  analyze,
  type Analysis,
  type SemToken,
  type SemTokenKind,
  type AozoraDiagnostic,
  type DiagnosticSeverity,
  type OutlineSymbol,
} from '@/commands/tauri'

/** 0 起点(line,ch) → CodeMirror の絶対位置。範囲外は行末/文末にクランプ。 */
export function toPos(doc: Text, line0: number, ch: number): number {
  const lineNo = Math.min(Math.max(line0, 0) + 1, doc.lines)
  const line = doc.line(lineNo)
  return Math.min(line.from + Math.max(ch, 0), line.to)
}

// --- 解析結果を保持する StateField（ハイライト・アウトライン用）-----------------
export const setAnalysisEffect = StateEffect.define<Analysis>()

export const analysisField = StateField.define<Analysis | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setAnalysisEffect)) return e.value
    return value
  },
})

/** 現在のアウトライン（見出し一覧）を取り出す。パネル描画などに使う。 */
export function getOutline(view: EditorView): OutlineSymbol[] {
  return view.state.field(analysisField, false)?.symbols ?? []
}

// --- セマンティックハイライト ----------------------------------------------------
const TOKEN_CLASS: Record<SemTokenKind, string> = {
  ruby: 'cm-aoz-ruby',
  heading: 'cm-aoz-heading',
  emphasis: 'cm-aoz-emphasis',
  gaiji: 'cm-aoz-gaiji',
  accent: 'cm-aoz-accent',
  image: 'cm-aoz-image',
  annotation: 'cm-aoz-annotation',
}

/** トークン列から装飾セットを作る（範囲外・空はスキップ、from 順にソート）。 */
export function buildTokenDecorations(doc: Text, tokens: SemToken[]): DecorationSet {
  const marks = tokens
    .map((t) => ({
      from: toPos(doc, t.range.line, t.range.start),
      to: toPos(doc, t.range.line, t.range.end),
      cls: TOKEN_CLASS[t.kind] ?? TOKEN_CLASS.annotation,
    }))
    .filter((m) => m.to > m.from)
    .sort((a, b) => a.from - b.from || a.to - b.to)

  const builder = new RangeSetBuilder<Decoration>()
  for (const m of marks) builder.add(m.from, m.to, Decoration.mark({ class: m.cls }))
  return builder.finish()
}

const highlightPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet
    constructor(view: EditorView) {
      this.decorations = build(view)
    }
    update(u: ViewUpdate) {
      const prev = u.startState.field(analysisField, false)
      const curr = u.state.field(analysisField, false)
      if (curr !== prev) {
        // 新しい解析が届いた → 作り直す。
        this.decorations = build(u.view)
      } else if (u.docChanged) {
        // 解析待ちの間は編集に追従して位置だけ動かす（ちらつき防止）。
        this.decorations = this.decorations.map(u.changes)
      }
    }
  },
  { decorations: (v) => v.decorations }
)

function build(view: EditorView): DecorationSet {
  const a = view.state.field(analysisField, false)
  if (!a) return Decoration.none
  return buildTokenDecorations(view.state.doc, a.tokens)
}

// --- ホバー ---------------------------------------------------------------------
const KIND_LABEL: Record<SemTokenKind, string> = {
  ruby: 'ルビ',
  heading: '見出し',
  emphasis: '強調',
  gaiji: '外字',
  accent: 'アクセント',
  image: '画像',
  annotation: '注記',
}

/** 位置 pos を覆うトークンを探す（範囲は 0 起点→絶対位置に変換して判定）。 */
export function tokenAtPos(view: EditorView, pos: number): SemToken | null {
  const a = view.state.field(analysisField, false)
  if (!a) return null
  const doc = view.state.doc
  for (const t of a.tokens) {
    const from = toPos(doc, t.range.line, t.range.start)
    const to = toPos(doc, t.range.line, t.range.end)
    if (to > from && pos >= from && pos <= to) return t
  }
  return null
}

const hover = hoverTooltip((view, pos): Tooltip | null => {
  const t = tokenAtPos(view, pos)
  if (!t) return null
  const doc = view.state.doc
  const from = toPos(doc, t.range.line, t.range.start)
  const to = toPos(doc, t.range.line, t.range.end)
  const label = KIND_LABEL[t.kind] ?? t.kind
  const text = t.detail ? `${label} — ${t.detail}` : label
  return {
    pos: from,
    end: to,
    above: true,
    create() {
      const dom = document.createElement('div')
      dom.className = 'cm-aoz-hover'
      dom.textContent = text
      return { dom }
    },
  }
})

// --- 診断 -----------------------------------------------------------------------
const SEVERITY: Record<DiagnosticSeverity, CmDiagnostic['severity']> = {
  error: 'error',
  warning: 'warning',
  info: 'info',
  hint: 'hint',
}

/** Rust の診断を CodeMirror の Diagnostic に変換。 */
export function toCmDiagnostics(doc: Text, diags: AozoraDiagnostic[]): CmDiagnostic[] {
  return diags
    .map((d) => ({
      from: toPos(doc, d.range.line, d.range.start),
      to: toPos(doc, d.range.line, d.range.end),
      severity: SEVERITY[d.severity] ?? 'warning',
      message: d.message,
      source: d.code,
    }))
    .filter((d) => d.to >= d.from)
}

// --- 解析ランナー（デバウンス）--------------------------------------------------
function analysisRunner(delayMs: number): Extension {
  let timer: ReturnType<typeof setTimeout> | undefined

  const run = (view: EditorView) => {
    const text = view.state.doc.toString()
    analyze(text)
      .then((a) => {
        // ハイライト/アウトライン用の保持と診断セットを 1 トランザクションにまとめる
        // （dispatch を 2 回するとエディタ更新も 2 回走るため）。
        view.dispatch(setDiagnostics(view.state, toCmDiagnostics(view.state.doc, a.diagnostics)), {
          effects: setAnalysisEffect.of(a),
        })
      })
      .catch(() => {
        // Tauri 非接続（素の vite 等）では解析不可。支援なしで動作継続。
      })
  }

  return EditorView.updateListener.of((u) => {
    if (u.docChanged) {
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => run(u.view), delayMs)
    }
  })
}

// --- アウトライン（見出しジャンプ・上部パネル）----------------------------------
// エディタ上部に見出し一覧の select を出し、選ぶとその行へスクロールする。
// レイアウトを変えない自己完結パネル。データは analysisField.symbols。
function outlinePanel(view: EditorView): Panel {
  const dom = document.createElement('div')
  dom.className = 'cm-aoz-outline'

  const label = document.createElement('span')
  label.className = 'cm-aoz-outline-label'
  label.textContent = '見出し'

  const select = document.createElement('select')
  select.className = 'cm-aoz-outline-select'

  dom.appendChild(label)
  dom.appendChild(select)

  const rebuild = (v: EditorView) => {
    const symbols = getOutline(v)
    select.textContent = ''
    const head = document.createElement('option')
    head.value = ''
    head.textContent = symbols.length ? `（${symbols.length} 件）へジャンプ…` : '（見出しなし）'
    select.appendChild(head)
    symbols.forEach((s, i) => {
      const o = document.createElement('option')
      o.value = String(i)
      // レベルでインデント（大=0, 中=1, 小=2）
      o.textContent = '　'.repeat(Math.max(0, s.level - 1)) + s.text
      select.appendChild(o)
    })
    select.disabled = symbols.length === 0
  }
  rebuild(view)

  select.addEventListener('change', () => {
    const i = Number(select.value)
    const symbols = getOutline(view)
    if (Number.isInteger(i) && symbols[i]) {
      const pos = toPos(view.state.doc, symbols[i].range.line, 0)
      view.dispatch({
        selection: { anchor: pos },
        effects: EditorView.scrollIntoView(pos, { y: 'center' }),
      })
      view.focus()
    }
    select.value = ''
  })

  return {
    dom,
    top: true,
    update(u: ViewUpdate) {
      if (u.state.field(analysisField, false) !== u.startState.field(analysisField, false)) {
        rebuild(u.view)
      }
    },
  }
}

/** アウトライン（見出しジャンプ）パネル拡張。テスト用に公開。 */
export const outline = showPanel.of(outlinePanel)

// --- 折りたたみ -----------------------------------------------------------------
// analysisField.folds（複数行ブロックの範囲）を CodeMirror の折りたたみに供給する。
const folding = [
  codeFolding(),
  foldGutter(),
  foldService.of((state, lineStart) => {
    const a = state.field(analysisField, false)
    if (!a) return null
    const line0 = state.doc.lineAt(lineStart).number - 1
    const fold = a.folds.find((f) => f.start_line === line0)
    if (!fold) return null
    const start = state.doc.line(Math.min(fold.start_line + 1, state.doc.lines))
    const end = state.doc.line(Math.min(fold.end_line + 1, state.doc.lines))
    // 開始行の末尾から終了行の末尾までを畳む（開始行だけ残す）。
    return end.to > start.to ? { from: start.to, to: end.to } : null
  }),
]

// --- ハイライト/診断の見た目 -----------------------------------------------------
const lspTheme = EditorView.theme({
  '.cm-aoz-ruby': { color: '#6c9249' },
  '.cm-aoz-heading': { color: '#3a6ea5', fontWeight: 'bold' },
  '.cm-aoz-emphasis': { color: '#8a5cc0' },
  '.cm-aoz-gaiji': { color: '#b8860b' },
  '.cm-aoz-accent': { color: '#b8860b' },
  '.cm-aoz-image': { color: '#2a8a8a' },
  '.cm-aoz-annotation': { color: '#5a7a9e' },
  '.cm-aoz-hover': {
    padding: '4px 8px',
    fontFamily: 'sans-serif',
    fontSize: '0.85rem',
    maxWidth: '32em',
  },
  '.cm-aoz-outline': {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '4px 8px',
    fontFamily: 'sans-serif',
    fontSize: '0.8rem',
    borderBottom: '1px solid var(--border)',
  },
  '.cm-aoz-outline-label': { opacity: '0.7' },
  '.cm-aoz-outline-select': {
    flex: '1',
    maxWidth: '28em',
    fontSize: '0.8rem',
    padding: '2px 4px',
  },
})

/**
 * 青空文庫 LSP 拡張一式（診断＋セマンティックハイライト）。
 * baseExtensions に展開して使う。delayMs は解析デバウンス（既定 200ms）。
 */
export function aozoraLsp(delayMs = 400): Extension[] {
  return [
    analysisField,
    highlightPlugin,
    hover,
    outline,
    ...folding,
    lspTheme,
    lintGutter(),
    analysisRunner(delayMs),
  ]
}
