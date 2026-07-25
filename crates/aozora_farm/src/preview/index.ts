import { convertToHtml, rewriteGaijiPaths, rewriteCssPaths, injectBaseTag } from '@/commands/tauri'
import { t } from '@/i18n'

let previewFrame: HTMLIFrameElement | null = null
let currentHtml = ''
let currentBaseDir = ''

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

    previewFrame.srcdoc = processedHtml
  } catch (error) {
    console.error('Conversion error:', error)
    previewFrame.srcdoc = `<html><body><p style="color: red;">${t('error.convert', { error: String(error) })}</p></body></html>`
  }
}

function showEmptyMessage(isInitial: boolean): void {
  if (!previewFrame) return

  const key = isInitial ? 'preview.empty-initial' : 'preview.empty'
  previewFrame.srcdoc = `<html><body><p style="color: #999;">${t(key)}</p></body></html>`
}
