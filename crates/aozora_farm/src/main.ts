import {
  createEditor,
  getContent,
  setContent,
  getEditor,
  openSearch,
  openReplace,
  setLiveAnalysis,
  analyzeNow,
} from '@/editor'
import { initPreview, updatePreview, setBaseDir, getHtml } from '@/preview'
import { t, updateUI, toggleLang, onLangChange } from '@/i18n'
import type { SaveSjisError } from '@/commands/tauri'
import {
  initResourcePaths,
  openTextFile,
  saveTextFile,
  saveTextFileSjis,
  saveHtmlFile,
  getDirectory,
  getFilename,
} from '@/commands/tauri'
import { Menu, MenuItem, PredefinedMenuItem, Submenu } from '@tauri-apps/api/menu'
import { getVersion } from '@tauri-apps/api/app'

// State
let currentFilename = ''
let _currentFilePath = ''  // Prefixed with _ to indicate unused for now
let debounceTimer: number | null = null
let appVersion = ''

// Elements
const editorContainer = document.getElementById('editor-container')!
const previewFrame = document.getElementById('preview-frame') as HTMLIFrameElement
const fileInfo = document.getElementById('file-info')!
const status = document.getElementById('status')!
const openFileBtn = document.getElementById('open-file')!
const saveTextBtn = document.getElementById('save-text')!
const saveTextSjisBtn = document.getElementById('save-text-sjis')!
const viewHtmlBtn = document.getElementById('view-html')!
const copyHtmlBtn = document.getElementById('copy-html')!
const saveHtmlBtn = document.getElementById('save-html')!
const liveToggle = document.getElementById('live-toggle') as HTMLInputElement
const refreshBtn = document.getElementById('refresh-preview')!

// ライブ更新（編集追従の自動変換＋自動解析）の有効/無効。大きな文書では OFF にして
// 手動更新にすると、入力停止後の周期的な重さを避けられる。
let liveEnabled = true
const htmlModal = document.getElementById('html-modal')!
const htmlSource = document.getElementById('html-source')!
const closeModalBtn = document.getElementById('close-modal')!
const langToggleBtn = document.getElementById('lang-toggle')!

// About modal elements
const aboutModal = document.getElementById('about-modal')!
const aboutTitle = document.getElementById('about-title')!
const aboutVersion = document.getElementById('about-version')!
const aboutDescription = document.getElementById('about-description')!
const closeAboutBtn = document.getElementById('close-about')!

// Update about modal content
function updateAboutModal(): void {
  aboutTitle.textContent = t('about.title')
  aboutVersion.textContent = t('about.version', { version: appVersion })
  // Replace \n with <br> for description
  aboutDescription.innerHTML = t('about.description').replace(/\n/g, '<br>')
}

// Show about dialog
function showAbout(): void {
  aboutModal.classList.add('show')
}

// Close about dialog
function closeAbout(): void {
  aboutModal.classList.remove('show')
}

// Setup application menu
//
// setAsAppMenu は Rust 側の既定メニュー（tauri は macOS で Menu::default を自動で
// 入れる）を置き換えるので、コピー・ペースト等の標準項目もここで並べる必要がある。
// macOS では Cmd+C/V/X はメニュー項目のキーエクイバレント経由で webview に届くため、
// 項目が無いとショートカット自体が効かない。
// 取り消す／やり直すは CodeMirror の history が beforeinput（historyUndo/historyRedo）
// を拾うので、ネイティブ項目からでもエディタの履歴が動く。
async function setupMenu(): Promise<void> {
  // App submenu
  const appSubmenu = await Submenu.new({
    text: t('app.title'),
    items: [
      await MenuItem.new({
        id: 'about',
        text: t('menu.about'),
        action: () => showAbout(),
      }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await PredefinedMenuItem.new({ item: 'Quit', text: t('menu.quit') }),
    ],
  })

  // File submenu
  const fileSubmenu = await Submenu.new({
    text: t('menu.file'),
    items: [
      await MenuItem.new({
        id: 'open_file',
        text: t('btn.open-file'),
        accelerator: 'CmdOrCtrl+O',
        action: () => openFile(),
      }),
      await MenuItem.new({
        id: 'save_text',
        text: t('btn.save-text'),
        accelerator: 'CmdOrCtrl+S',
        action: () => saveText(),
      }),
    ],
  })

  // Edit submenu
  const editSubmenu = await Submenu.new({
    text: t('menu.edit'),
    items: [
      await PredefinedMenuItem.new({ item: 'Undo', text: t('menu.undo') }),
      await PredefinedMenuItem.new({ item: 'Redo', text: t('menu.redo') }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await PredefinedMenuItem.new({ item: 'Cut', text: t('menu.cut') }),
      await PredefinedMenuItem.new({ item: 'Copy', text: t('menu.copy') }),
      await PredefinedMenuItem.new({ item: 'Paste', text: t('menu.paste') }),
      await PredefinedMenuItem.new({ item: 'SelectAll', text: t('menu.select-all') }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({
        id: 'search',
        text: t('menu.find'),
        accelerator: 'CmdOrCtrl+F',
        action: () => openSearch(),
      }),
      await MenuItem.new({
        id: 'replace',
        text: t('menu.replace'),
        accelerator: 'CmdOrCtrl+Shift+F',
        action: () => openReplace(),
      }),
    ],
  })

  // View submenu
  const viewSubmenu = await Submenu.new({
    text: t('menu.view'),
    items: [await PredefinedMenuItem.new({ item: 'Fullscreen', text: t('menu.fullscreen') })],
  })

  // Window submenu
  const windowSubmenu = await Submenu.new({
    text: t('menu.window'),
    items: [
      await PredefinedMenuItem.new({ item: 'Minimize', text: t('menu.minimize') }),
      await PredefinedMenuItem.new({ item: 'Maximize', text: t('menu.zoom') }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await PredefinedMenuItem.new({ item: 'CloseWindow', text: t('menu.close-window') }),
    ],
  })

  const menu = await Menu.new({
    items: [appSubmenu, fileSubmenu, editSubmenu, viewSubmenu, windowSubmenu],
  })

  await menu.setAsAppMenu()
}

// Initialize
async function init(): Promise<void> {
  // Initialize resource paths
  await initResourcePaths()

  // Get app version
  appVersion = await getVersion()

  // Setup application menu
  await setupMenu()

  // Initialize preview
  initPreview(previewFrame)

  // Create editor with change callback
  createEditor(editorContainer, debouncedConvert)

  // Setup event listeners
  setupEventListeners()

  // Update UI with translations
  updateUI()

  // Update about modal content
  updateAboutModal()

  // Listen for language changes to update about modal
  onLangChange(() => updateAboutModal())

  // ネイティブメニューはラベルを差し替えられないので、言語切替のたびに組み直す。
  onLangChange(() => void setupMenu())
}

// Debounced convert
function debouncedConvert(content: string): void {
  // ライブ OFF 時は編集で自動更新しない（プレビュー変換も走らせない）。
  if (!liveEnabled) return
  if (debounceTimer) {
    clearTimeout(debounceTimer)
  }
  // 大きな文書ほどプレビュー変換の頻度を下げる（base 500ms + 長さ比例、上限 2000ms）。
  const delay = Math.min(500 + Math.floor(content.length / 1000), 2000)
  debounceTimer = window.setTimeout(async () => {
    setStatus(t('status.converting'), '')
    await updatePreview(content)
    if (content.trim()) {
      setStatus(t('status.converted'), 'success')
    } else {
      setStatus(t('status.ready'), '')
    }
  }, delay)
}

// Set status message
function setStatus(message: string, type: string = ''): void {
  status.textContent = message
  status.className = 'status ' + type
}

// Open file
async function openFile(): Promise<void> {
  try {
    const result = await openTextFile()
    if (!result) return

    setContent(result.content)
    _currentFilePath = result.path
    currentFilename = getFilename(result.path)
    setBaseDir(getDirectory(result.path))
    fileInfo.textContent = t('file.info', { filename: currentFilename })

    // Trigger conversion
    await updatePreview(result.content)
    setStatus(t('status.converted'), 'success')
  } catch (error) {
    setStatus(t('error.open-file', { error: String(error) }), 'error')
  }
}

// Save text
async function saveText(): Promise<void> {
  const content = getContent()
  if (!content.trim()) {
    setStatus(t('status.no-text'), 'error')
    return
  }

  try {
    const defaultName = currentFilename || 'untitled.txt'
    const savePath = await saveTextFile(content, defaultName)

    if (savePath) {
      _currentFilePath = savePath
      currentFilename = getFilename(savePath)
      setBaseDir(getDirectory(savePath))
      fileInfo.textContent = t('file.info', { filename: currentFilename })
      setStatus(t('status.text-saved', { path: savePath }), 'success')
    }
  } catch (error) {
    setStatus(t('error.save-file', { error: String(error) }), 'error')
  }
}

// Save text as Shift_JIS
//
// 青空文庫のファイルは Shift_JIS ＋ CRLF。変換は Rust 側（save_text_sjis）に任せる。
// Shift_JIS にできない文字があるときは保存せず、直すべき箇所を位置つきで知らせる。
async function saveTextSjis(): Promise<void> {
  const content = getContent()
  if (!content.trim()) {
    setStatus(t('status.no-text'), 'error')
    return
  }

  try {
    const defaultName = currentFilename || 'untitled.txt'
    const savePath = await saveTextFileSjis(content, defaultName)

    if (savePath) {
      _currentFilePath = savePath
      currentFilename = getFilename(savePath)
      setBaseDir(getDirectory(savePath))
      fileInfo.textContent = t('file.info', { filename: currentFilename })
      setStatus(t('status.text-saved-sjis', { path: savePath }), 'success')
    }
  } catch (error) {
    setStatus(describeSaveSjisError(error), 'error')
  }
}

/** 保存できなかった理由を1行で説明する。符号化できない文字は先頭 3 件まで挙げる。 */
function describeSaveSjisError(error: unknown): string {
  const detail = error as SaveSjisError | undefined
  if (detail?.kind !== 'unencodable') {
    return t('error.save-file', { error: String(detail?.kind === 'io' ? detail.message : error) })
  }

  const shown = detail.chars
    .slice(0, 3)
    .map((c) => t('error.sjis-char', { char: c.ch, line: String(c.line + 1), column: String(c.column + 1) }))
    .join('、')
  const rest = detail.chars.length - 3
  return rest > 0
    ? t('error.sjis-unencodable-more', { chars: shown, rest: String(rest) })
    : t('error.sjis-unencodable', { chars: shown })
}

// Save HTML
async function saveHtml(): Promise<void> {
  const html = getHtml()
  if (!html) {
    setStatus(t('status.no-html', { action: t('btn.save-html') }), 'error')
    return
  }

  try {
    const defaultName = currentFilename
      ? currentFilename.replace(/\.txt$/i, '.html')
      : 'output.html'

    const savePath = await saveHtmlFile(html, defaultName)

    if (savePath) {
      setStatus(t('status.saved', { path: savePath }), 'success')
    }
  } catch (error) {
    // HTML も Shift_JIS で書くので、符号化できない文字の報せ方はテキスト保存と同じ。
    setStatus(describeSaveSjisError(error), 'error')
  }
}

// Copy HTML to clipboard
async function copyHtml(): Promise<void> {
  const html = getHtml()
  if (!html) {
    setStatus(t('status.no-html', { action: t('btn.copy-html') }), 'error')
    return
  }

  try {
    await navigator.clipboard.writeText(html)
    setStatus(t('status.copied'), 'success')
  } catch (error) {
    setStatus(t('error.copy', { error: String(error) }), 'error')
  }
}

// View HTML source
function viewHtml(): void {
  const html = getHtml()
  if (!html) {
    setStatus(t('status.no-html', { action: t('btn.view-html') }), 'error')
    return
  }

  htmlSource.textContent = html
  htmlModal.classList.add('show')
}

// Close modal
function closeModal(): void {
  htmlModal.classList.remove('show')
}

// Setup event listeners
// 手動更新: 現在の内容でプレビュー変換と解析を今すぐ一括実行する。
async function refreshNow(): Promise<void> {
  const content = getContent()
  const ed = getEditor()
  if (ed) analyzeNow(ed)
  setStatus(t('status.converting'), '')
  await updatePreview(content)
  setStatus(content.trim() ? t('status.converted') : t('status.ready'), content.trim() ? 'success' : '')
}

function setLive(on: boolean): void {
  liveEnabled = on
  setLiveAnalysis(on)
  liveToggle.checked = on
  // ON に戻したら即座に最新へ揃える。
  if (on) void refreshNow()
}

function setupEventListeners(): void {
  openFileBtn.addEventListener('click', openFile)
  refreshBtn.addEventListener('click', () => void refreshNow())
  liveToggle.addEventListener('change', () => setLive(liveToggle.checked))
  saveTextBtn.addEventListener('click', saveText)
  saveTextSjisBtn.addEventListener('click', saveTextSjis)
  viewHtmlBtn.addEventListener('click', viewHtml)
  copyHtmlBtn.addEventListener('click', copyHtml)
  saveHtmlBtn.addEventListener('click', saveHtml)
  closeModalBtn.addEventListener('click', closeModal)
  closeAboutBtn.addEventListener('click', closeAbout)
  langToggleBtn.addEventListener('click', () => toggleLang())

  // Close modal on backdrop click
  htmlModal.addEventListener('click', (e) => {
    if (e.target === htmlModal) {
      closeModal()
    }
  })

  aboutModal.addEventListener('click', (e) => {
    if (e.target === aboutModal) {
      closeAbout()
    }
  })

  // Global keyboard shortcuts
  document.addEventListener('keydown', (e) => {
    // Escape to close modal
    if (e.key === 'Escape') {
      if (htmlModal.classList.contains('show')) {
        closeModal()
      }
      if (aboutModal.classList.contains('show')) {
        closeAbout()
      }
    }
  })

  // Drag and drop
  const editor = getEditor()
  if (editor) {
    editor.dom.addEventListener('dragover', (e) => {
      e.preventDefault()
    })

    editor.dom.addEventListener('drop', async (e) => {
      e.preventDefault()

      const files = e.dataTransfer?.files
      if (!files || files.length === 0) return

      const file = files[0]
      if (!file.name.endsWith('.txt')) {
        setStatus(t('status.drop-txt'), 'error')
        return
      }

      try {
        const arrayBuffer = await file.arrayBuffer()
        const content = decodeText(new Uint8Array(arrayBuffer))
        setContent(content)
        currentFilename = file.name
        fileInfo.textContent = t('file.info', { filename: currentFilename })
        await updatePreview(content)
        setStatus(t('status.converted'), 'success')
      } catch (error) {
        setStatus(t('error.read-file', { error: String(error) }), 'error')
      }
    })
  }

  // Prevent default drag behavior on window
  window.addEventListener('dragover', (e) => e.preventDefault())
  window.addEventListener('drop', (e) => e.preventDefault())
}

// Decode text with encoding detection
function decodeText(bytes: Uint8Array): string {
  try {
    const utf8Decoder = new TextDecoder('utf-8', { fatal: true })
    return utf8Decoder.decode(bytes)
  } catch {
    const sjisDecoder = new TextDecoder('shift_jis')
    return sjisDecoder.decode(bytes)
  }
}

// Start the app
init()
