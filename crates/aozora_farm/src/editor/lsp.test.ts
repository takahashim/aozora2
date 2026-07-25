import { describe, it, expect } from 'vitest'
import { Text, EditorState } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import {
  toPos,
  toCmDiagnostics,
  buildTokenDecorations,
  tokenAtPos,
  analysisField,
  setAnalysisEffect,
} from './lsp'
import type { SemToken, AozoraDiagnostic } from '@/commands/tauri'

const doc = Text.of(['一行目', '東京《とうきょう》', '三行目'])

describe('toPos', () => {
  it('0起点(line,ch) を絶対位置に変換する', () => {
    // 1行目=「一行目」(0..3, from=0), 2行目 from=4。
    expect(toPos(doc, 0, 0)).toBe(0)
    expect(toPos(doc, 0, 2)).toBe(2)
    expect(toPos(doc, 1, 0)).toBe(4) // "一行目\n" の次
    expect(toPos(doc, 1, 2)).toBe(6)
  })

  it('範囲外は行末/文末にクランプする', () => {
    expect(toPos(doc, 0, 999)).toBe(3) // 1行目の行末
    expect(toPos(doc, 999, 0)).toBe(doc.line(doc.lines).from) // 最終行頭
    expect(toPos(doc, -1, -5)).toBe(0)
  })
})

describe('toCmDiagnostics', () => {
  it('severity・位置・メッセージを写し、code を source にする', () => {
    const diags: AozoraDiagnostic[] = [
      {
        range: { line: 1, start: 0, end: 4 },
        severity: 'warning',
        code: 'unresolved-reference',
        message: '注記の対象が見つかりません',
      },
    ]
    const cm = toCmDiagnostics(doc, diags)
    expect(cm).toHaveLength(1)
    expect(cm[0].severity).toBe('warning')
    expect(cm[0].from).toBe(4)
    expect(cm[0].to).toBe(8)
    expect(cm[0].source).toBe('unresolved-reference')
    expect(cm[0].message).toContain('注記')
  })

  it('4種の severity を CodeMirror にマップする', () => {
    const mk = (s: AozoraDiagnostic['severity']): AozoraDiagnostic => ({
      range: { line: 0, start: 0, end: 1 },
      severity: s,
      code: 'x',
      message: 'm',
    })
    const cm = toCmDiagnostics(doc, [mk('error'), mk('warning'), mk('info'), mk('hint')])
    expect(cm.map((d) => d.severity)).toEqual(['error', 'warning', 'info', 'hint'])
  })
})

describe('buildTokenDecorations', () => {
  it('トークンごとに装飾を作り、空範囲は捨てる', () => {
    const tokens: SemToken[] = [
      { range: { line: 1, start: 2, end: 9 }, kind: 'ruby', detail: 'ルビ: とうきょう' }, // 《とうきょう》
      { range: { line: 0, start: 0, end: 0 }, kind: 'annotation', detail: null }, // 空→除外
    ]
    const set = buildTokenDecorations(doc, tokens)
    expect(set.size).toBe(1)
  })

  it('from 順にソートして追加する（順不同入力でも壊れない）', () => {
    const tokens: SemToken[] = [
      { range: { line: 2, start: 0, end: 2 }, kind: 'heading', detail: null },
      { range: { line: 0, start: 0, end: 2 }, kind: 'emphasis', detail: null },
      { range: { line: 1, start: 2, end: 9 }, kind: 'ruby', detail: null },
    ]
    const set = buildTokenDecorations(doc, tokens)
    expect(set.size).toBe(3)
    const froms: number[] = []
    const cur = set.iter()
    while (cur.value) {
      froms.push(cur.from)
      cur.next()
    }
    expect(froms).toEqual([...froms].sort((a, b) => a - b))
  })
})

describe('tokenAtPos', () => {
  it('位置を覆うトークンを返し、外れると null', () => {
    const view = new EditorView({
      state: EditorState.create({
        doc: '東京《とうきょう》',
        extensions: [analysisField],
      }),
    })
    view.dispatch({
      effects: setAnalysisEffect.of({
        tokens: [{ range: { line: 0, start: 2, end: 9 }, kind: 'ruby', detail: 'ルビ: とうきょう' }],
        symbols: [],
        diagnostics: [],
      }),
    })
    // 《…》の内側（絶対 2..9）
    expect(tokenAtPos(view, 5)?.kind).toBe('ruby')
    expect(tokenAtPos(view, 5)?.detail).toBe('ルビ: とうきょう')
    // base「東京」の位置（0..2）はトークン外
    expect(tokenAtPos(view, 1)).toBeNull()
    view.destroy()
  })
})

describe('outline panel', () => {
  it('見出しシンボルを select の option に反映する', async () => {
    const { outline, setAnalysisEffect, analysisField } = await import('./lsp')
    const parent = document.createElement('div')
    document.body.appendChild(parent)
    const view = new EditorView({
      state: EditorState.create({ doc: '序章\n本文\n第二章', extensions: [analysisField, outline] }),
      parent,
    })
    view.dispatch({
      effects: setAnalysisEffect.of({
        tokens: [],
        symbols: [
          { range: { line: 0, start: 0, end: 2 }, level: 1, text: '序章' },
          { range: { line: 2, start: 0, end: 3 }, level: 2, text: '第二章' },
        ],
        diagnostics: [],
      }),
    })
    const select = view.dom.querySelector('.cm-aoz-outline-select') as HTMLSelectElement
    expect(select).toBeTruthy()
    // 先頭のプレースホルダ + 見出し2件
    expect(select.options.length).toBe(3)
    expect(select.options[1].textContent).toContain('序章')
    expect(select.options[2].textContent).toContain('第二章')
    view.destroy()
    parent.remove()
  })
})
