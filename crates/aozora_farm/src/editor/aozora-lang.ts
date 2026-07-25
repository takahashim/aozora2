import { StreamLanguage } from '@codemirror/language'

// 青空文庫記法のシンプルなトークナイザー
export const aozoraLanguage = StreamLanguage.define({
  token(stream) {
    // ルビ記法 《...》
    if (stream.match(/《[^》]*》/)) {
      return 'string'
    }
    // 注記 ［＃...］
    if (stream.match(/［＃[^］]*］/)) {
      return 'keyword'
    }
    // ルビ開始記号
    if (stream.match(/｜/)) {
      return 'operator'
    }
    // 外字記号
    if (stream.match(/※/)) {
      return 'atom'
    }
    // その他の文字
    stream.next()
    return null
  }
})
