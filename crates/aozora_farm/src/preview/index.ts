import { convertToHtml, rewriteGaijiPaths, rewriteCssPaths, injectBaseTag } from '@/commands/tauri'
import { t } from '@/i18n'

let previewFrame: HTMLIFrameElement | null = null
let currentHtml = ''
let currentBaseDir = ''
// iframe に完全な文書骨格（head の CSS など）が読み込み済みか。読み込み済みなら
// 以降の更新は <body> の中身だけ差し替え、head や CSS の再読込を避けて軽くする。
let docReady = false

export function initPreview(iframe: HTMLIFrameElement): void {
  previewFrame = iframe
  showEmptyMessage(true)
}

export function setBaseDir(dir: string): void {
  currentBaseDir = dir
}

export function getHtml(): string {
  return currentHtml
}

export async function updatePreview(content: string): Promise<void> {
  if (!previewFrame) return

  if (!content.trim()) {
    currentHtml = ''
    showEmptyMessage(false)
    return
  }

  try {
    const html = await convertToHtml(content)
    currentHtml = html

    // Rewrite asset paths to use bundled resources
    let processedHtml = rewriteGaijiPaths(html)
    processedHtml = rewriteCssPaths(processedHtml)
    // Inject base tag for relative image paths
    processedHtml = injectBaseTag(processedHtml, currentBaseDir)

    const doc = previewFrame.contentDocument
    if (docReady && doc && doc.body) {
      // 差分更新: <body> の中身だけ差し替える。<head>（CSS）は再読込しないので、
      // iframe 全体を srcdoc で作り直すより大幅に軽い。スクロール位置も保たれる。
      const parsed = new DOMParser().parseFromString(processedHtml, 'text/html')
      doc.body.innerHTML = parsed.body.innerHTML
    } else {
      // 初回（または空表示・エラー後の復帰）は文書全体を読み込む。読み込み完了で
      // 差分更新に切り替える。
      loadFullDocument(processedHtml)
    }
  } catch (error) {
    console.error('Conversion error:', error)
    loadFullDocument(
      `<html><body><p style="color: red;">${t('error.convert', { error: String(error) })}</p></body></html>`
    )
  }
}

/** iframe に文書全体を読み込む。読み込み完了後、以降は body 差分更新に切り替える。 */
function loadFullDocument(html: string): void {
  if (!previewFrame) return
  docReady = false
  previewFrame.addEventListener(
    'load',
    () => {
      docReady = true
    },
    { once: true }
  )
  previewFrame.srcdoc = html
}

function showEmptyMessage(isInitial: boolean): void {
  if (!previewFrame) return

  const key = isInitial ? 'preview.empty-initial' : 'preview.empty'
  loadFullDocument(`<html><body><p style="color: #999;">${t(key)}</p></body></html>`)
}
