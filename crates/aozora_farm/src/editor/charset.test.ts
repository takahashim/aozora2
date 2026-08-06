import { describe, it, expect } from 'vitest'
import { createLookup, scanText, type CharsetLookup } from './charset'
import { MARK_OTHER, MARK_PLAIN, MARK_X0201, MARK_X0213, type CharsetTable } from '@/commands/tauri'

// Rust の mark_table と同じ詰め方で、テストに必要な文字だけ入れた表を作る。
function table(marks: Record<number, number>, extra: Partial<CharsetTable> = {}): CharsetTable {
  const bmp = new Array(0x10000 / 4).fill(0)
  for (const [cp, mark] of Object.entries(marks)) {
    const i = Number(cp)
    bmp[i >> 2] |= mark << ((i & 3) * 2)
  }
  return { bmp, astral: [], composed: [], ...extra }
}

// 本文でよく出る文字の区分。実際の値は Rust 側のテストが規格と突き合わせている。
const lookup: CharsetLookup = createLookup(
  table(
    {
      0x3042: MARK_PLAIN, // あ
      0x306e: MARK_PLAIN, // の
      0x6f22: MARK_PLAIN, // 漢
      0x5b57: MARK_PLAIN, // 字
      0xff71: MARK_X0201, // ｱ
      0x2460: MARK_X0213, // ① 第3水準 1-13-01
      0x304b: MARK_PLAIN, // か
      0x309a: MARK_OTHER, // 結合半濁点（単独では JIS 外）
    },
    { astral: [0x20b9f], composed: ['か゚'] }
  )
)

describe('createLookup', () => {
  it('ASCII は表を引かずに無色', () => {
    expect(lookup.markAt(0x41)).toBe(MARK_PLAIN)
  })

  it('BMP はビットマップを引く', () => {
    expect(lookup.markAt(0x3042)).toBe(MARK_PLAIN)
    expect(lookup.markAt(0xff71)).toBe(MARK_X0201)
    expect(lookup.markAt(0x2460)).toBe(MARK_X0213)
  })

  it('BMP の外は一覧にあれば第3・第4水準、無ければ JIS 外', () => {
    expect(lookup.markAt(0x20b9f)).toBe(MARK_X0213) // 𠮟
    expect(lookup.markAt(0x1f600)).toBe(MARK_OTHER) // 😀
  })
})

describe('scanText', () => {
  it('普通の本文には何も付けない', () => {
    expect(scanText('あの漢字', lookup)).toEqual([])
  })

  it('区分ごとに範囲を返す', () => {
    // 「漢ｱ①」→ 1 文字目は無色、2・3 文字目はそれぞれ別区分
    expect(scanText('漢ｱ①', lookup)).toEqual([
      { from: 1, to: 2, mark: MARK_X0201 },
      { from: 2, to: 3, mark: MARK_X0213 },
    ])
  })

  it('隣り合う同区分はひとつにまとめる', () => {
    expect(scanText('ｱｱｱ', lookup)).toEqual([{ from: 0, to: 3, mark: MARK_X0201 }])
  })

  it('サロゲートペアを 1 文字として扱う', () => {
    const text = '漢\u{20B9F}字' // 𠮟 は 2 コード単位
    expect(scanText(text, lookup)).toEqual([{ from: 1, to: 3, mark: MARK_X0213 }])
  })

  it('仮名＋結合半濁点は組で 1 文字にする', () => {
    // か単独なら無色、゚単独なら JIS 外。組なら第3水準として 2 コード単位まとめて色を付ける。
    expect(scanText('か゚', lookup)).toEqual([{ from: 0, to: 2, mark: MARK_X0213 }])
    expect(scanText('か', lookup)).toEqual([])
    expect(scanText('゚', lookup)).toEqual([{ from: 0, to: 1, mark: MARK_OTHER }])
  })

  it('JIS 外の文字に色を付ける', () => {
    expect(scanText('😀', lookup)).toEqual([{ from: 0, to: 2, mark: MARK_OTHER }])
  })
})
