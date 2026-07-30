import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { open, save } from '@tauri-apps/plugin-dialog'
import { readFile, writeTextFile } from '@tauri-apps/plugin-fs'

export interface ResourcePaths {
  gaiji: string
  css: string
}

let resourcePaths: ResourcePaths = { gaiji: '', css: '' }

export async function initResourcePaths(): Promise<void> {
  try {
    resourcePaths = await invoke<ResourcePaths>('get_resource_paths')
    console.log('Resource paths:', resourcePaths)
  } catch (e) {
    console.warn('Could not get resource paths:', e)
  }
}

export function getResourcePaths(): ResourcePaths {
  return resourcePaths
}

export async function convertToHtml(input: string): Promise<string> {
  return invoke<string>('convert_to_html', { input })
}

// --- 静的解析（LSP 的機能の土台）---------------------------------------
// Rust: aozora_core::analysis の各型に対応。位置は 0 起点（行・char）で
// end は含まない半開区間。CodeMirror へは line+1 して渡す。

/** 行内の char 範囲（0 起点・end 含まない）。line も 0 起点。 */
export interface AozoraRange {
  line: number
  start: number
  end: number
}

export type SemTokenKind =
  | 'ruby'
  | 'heading'
  | 'emphasis'
  | 'gaiji'
  | 'accent'
  | 'image'
  | 'annotation'

export interface SemToken {
  range: AozoraRange
  kind: SemTokenKind
  /** ホバー用の説明（外字の実文字・ルビ読みなど）。無ければ null。 */
  detail: string | null
}

export interface OutlineSymbol {
  range: AozoraRange
  /** 1=大, 2=中, 3=小 */
  level: number
  text: string
}

export type DiagnosticSeverity = 'error' | 'warning' | 'info' | 'hint'

export interface AozoraDiagnostic {
  range: AozoraRange
  severity: DiagnosticSeverity
  code: string
  message: string
}

export interface FoldRange {
  start_line: number
  end_line: number
}

export interface Analysis {
  tokens: SemToken[]
  symbols: OutlineSymbol[]
  diagnostics: AozoraDiagnostic[]
  folds: FoldRange[]
}

/** バッファ全体を解析してトークン／アウトライン／診断を得る。 */
export async function analyze(input: string): Promise<Analysis> {
  return invoke<Analysis>('analyze', { input })
}

export async function openTextFile(): Promise<{ content: string; path: string } | null> {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Text Files', extensions: ['txt'] }]
  })

  if (!selected) return null

  const bytes = await readFile(selected)
  const content = decodeText(bytes)
  return { content, path: selected }
}

export async function saveTextFile(content: string, defaultName: string): Promise<string | null> {
  const savePath = await save({
    defaultPath: defaultName,
    filters: [{ name: 'Text Files', extensions: ['txt'] }]
  })

  if (!savePath) return null

  await writeTextFile(savePath, content)
  return savePath
}

/** Shift_JIS で符号化できなかった文字（位置は 0 起点の行・char）。 */
export interface UnencodableChar {
  ch: string
  line: number
  column: number
}

export type SaveSjisError =
  | { kind: 'unencodable'; chars: UnencodableChar[] }
  | { kind: 'io'; message: string }

/**
 * Shift_JIS（CRLF）で保存する。改行の正規化・符号位置の正規化・符号化は Rust 側
 * （save_shift_jis）が行う。ブラウザの TextEncoder は UTF-8 しか出せない。
 *
 * Shift_JIS にできない文字があると保存せずに reject する（SaveSjisError）。
 */
async function saveWithSjis(
  content: string,
  defaultName: string,
  filter: { name: string; extensions: string[] }
): Promise<string | null> {
  const savePath = await save({ defaultPath: defaultName, filters: [filter] })

  if (!savePath) return null

  await invoke('save_shift_jis', { path: savePath, content })
  return savePath
}

/** 本文テキストを Shift_JIS で保存する。 */
export async function saveTextFileSjis(content: string, defaultName: string): Promise<string | null> {
  return saveWithSjis(content, defaultName, { name: 'Text Files', extensions: ['txt'] })
}

/**
 * 変換した HTML を保存する。出力は Shift_JIS。
 *
 * HTML の先頭は `<?xml version="1.0" encoding="Shift_JIS"?>` と
 * `charset=Shift_JIS` を宣言しているので、UTF-8 で書くと宣言と実バイト列が
 * 食い違い、宣言を尊重するブラウザで本文が文字化けする。
 */
export async function saveHtmlFile(content: string, defaultName: string): Promise<string | null> {
  return saveWithSjis(content, defaultName, { name: 'HTML Files', extensions: ['html'] })
}

export function toAssetUrl(path: string): string {
  return convertFileSrc(path)
}

// Decode text with encoding detection (Shift_JIS or UTF-8)
function decodeText(bytes: Uint8Array): string {
  // Try UTF-8 first
  try {
    const utf8Decoder = new TextDecoder('utf-8', { fatal: true })
    return utf8Decoder.decode(bytes)
  } catch {
    // Fall back to Shift_JIS
    const sjisDecoder = new TextDecoder('shift_jis')
    return sjisDecoder.decode(bytes)
  }
}

// Get directory from file path
export function getDirectory(filePath: string): string {
  const lastSlash = Math.max(filePath.lastIndexOf('/'), filePath.lastIndexOf('\\'))
  return lastSlash > 0 ? filePath.substring(0, lastSlash) : ''
}

// Get filename from file path
export function getFilename(filePath: string): string {
  return filePath.split('/').pop()?.split('\\').pop() || ''
}

// Rewrite gaiji image paths to use bundled assets
export function rewriteGaijiPaths(html: string): string {
  if (!resourcePaths.gaiji) return html

  const gaijiPattern = /src="([^"]*gaiji\/(\d+-\d+\/\d+-\d+-\d+\.png))"/g

  return html.replace(gaijiPattern, (_match, _fullPath, relativePath) => {
    const assetPath = `${resourcePaths.gaiji}/${relativePath}`
    const assetUrl = convertFileSrc(assetPath)
    return `src="${assetUrl}"`
  })
}

// Rewrite CSS paths to use bundled assets
export function rewriteCssPaths(html: string): string {
  if (!resourcePaths.css) return html

  const cssPattern = /href="([^"]*\/?(aozora\.css))"/g

  return html.replace(cssPattern, (_match, _fullPath, filename) => {
    const assetPath = `${resourcePaths.css}/${filename}`
    const assetUrl = convertFileSrc(assetPath)
    return `href="${assetUrl}"`
  })
}

// Inject base tag for relative image paths
export function injectBaseTag(html: string, baseDir: string): string {
  if (!baseDir) return html

  const baseUrl = convertFileSrc(baseDir) + '/'
  const baseTag = `<base href="${baseUrl}">`

  return html.replace(/<head>/i, `<head>\n${baseTag}`)
}
