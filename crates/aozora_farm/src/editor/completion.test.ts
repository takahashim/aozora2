import { describe, it, expect } from 'vitest'
import { filterSnippets, SNIPPETS } from './completion'

describe('filterSnippets', () => {
  it('空入力なら全件返す', () => {
    expect(filterSnippets('')).toHaveLength(SNIPPETS.length)
    expect(filterSnippets('  ')).toHaveLength(SNIPPETS.length)
  })

  it('漢字ラベルの部分一致で絞る', () => {
    const r = filterSnippets('傍点')
    expect(r.map((s) => s.label)).toContain('傍点')
    expect(r.map((s) => s.label)).toContain('二重傍点')
    expect(r.every((s) => s.label.includes('傍点'))).toBe(true)
  })

  it('読み（ひらがな）でも絞れる', () => {
    const r = filterSnippets('ふとじ')
    expect(r.map((s) => s.label)).toContain('太字')
  })

  it('該当なしは空', () => {
    expect(filterSnippets('存在しない語')).toHaveLength(0)
  })

  it('各候補の apply は ［＃ で始まり cursor は範囲内', () => {
    for (const s of SNIPPETS) {
      expect(s.apply.startsWith('［＃')).toBe(true)
      if (s.cursor !== undefined) {
        expect(s.cursor).toBeGreaterThanOrEqual(0)
        expect(s.cursor).toBeLessThanOrEqual(s.apply.length)
      }
    }
  })
})
