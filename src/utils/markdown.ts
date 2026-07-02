// src/utils/markdown.ts
// 轻量 Markdown → HTML 渲染（§5.1）。零依赖，覆盖常见语法即可（标题/粗斜体/行内码/代码块/
// 列表/引用/链接/分割线/段落）。先做 HTML 转义再套用变换，避免 XSS。
// Lightweight Markdown → HTML (§5.1). Dependency-free; covers common syntax. HTML is escaped
// first, then markdown transforms applied, so raw input can't inject markup.

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

// 行内：代码 `x` > 粗 **x** > 斜 *x* > 链接 [t](u)。先处理行内码占位，避免其内部再被转义破坏。
function renderInline(text: string): string {
  let out = escapeHtml(text)
  // 行内代码（先于其它，内部不再解析）
  out = out.replace(/`([^`]+)`/g, (_m, c) => `<code>${c}</code>`)
  // 粗体
  out = out.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  // 斜体（避开已消费的 **）
  out = out.replace(/(^|[^*])\*([^*\n]+)\*/g, '$1<em>$2</em>')
  // 链接 [text](url) —— 仅允许 http(s)/相对，禁止 javascript: 等协议
  out = out.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_m, t, u) => {
    const safe = /^(https?:\/\/|\/|#)/i.test(u) ? u : '#'
    return `<a href="${safe}" target="_blank" rel="noopener noreferrer">${t}</a>`
  })
  return out
}

export function renderMarkdown(src: string): string {
  const lines = src.replace(/\r\n?/g, '\n').split('\n')
  const html: string[] = []
  let inCode = false
  let codeBuf: string[] = []
  let listType: 'ul' | 'ol' | null = null
  let paraBuf: string[] = []

  const flushPara = () => {
    if (paraBuf.length) {
      html.push(`<p>${paraBuf.map(renderInline).join('<br>')}</p>`)
      paraBuf = []
    }
  }
  const closeList = () => {
    if (listType) {
      html.push(`</${listType}>`)
      listType = null
    }
  }

  for (const raw of lines) {
    // 代码块围栏 ```
    if (/^```/.test(raw.trim())) {
      if (inCode) {
        html.push(`<pre><code>${escapeHtml(codeBuf.join('\n'))}</code></pre>`)
        codeBuf = []
        inCode = false
      } else {
        flushPara()
        closeList()
        inCode = true
      }
      continue
    }
    if (inCode) {
      codeBuf.push(raw)
      continue
    }

    const line = raw.trimEnd()

    // 空行：段落/列表分隔
    if (!line.trim()) {
      flushPara()
      closeList()
      continue
    }

    // 标题 # … ######
    const h = /^(#{1,6})\s+(.*)$/.exec(line)
    if (h) {
      flushPara()
      closeList()
      const level = h[1].length
      html.push(`<h${level}>${renderInline(h[2])}</h${level}>`)
      continue
    }

    // 分割线
    if (/^(\*\*\*|---|___)\s*$/.test(line)) {
      flushPara()
      closeList()
      html.push('<hr>')
      continue
    }

    // 引用 >
    if (/^>\s?/.test(line)) {
      flushPara()
      closeList()
      html.push(`<blockquote>${renderInline(line.replace(/^>\s?/, ''))}</blockquote>`)
      continue
    }

    // 无序列表 - * +
    const ul = /^[-*+]\s+(.*)$/.exec(line)
    if (ul) {
      flushPara()
      if (listType !== 'ul') {
        closeList()
        listType = 'ul'
        html.push('<ul>')
      }
      html.push(`<li>${renderInline(ul[1])}</li>`)
      continue
    }

    // 有序列表 1. 2. …
    const ol = /^\d+\.\s+(.*)$/.exec(line)
    if (ol) {
      flushPara()
      if (listType !== 'ol') {
        closeList()
        listType = 'ol'
        html.push('<ol>')
      }
      html.push(`<li>${renderInline(ol[1])}</li>`)
      continue
    }

    // 普通段落行
    closeList()
    paraBuf.push(line)
  }

  if (inCode) html.push(`<pre><code>${escapeHtml(codeBuf.join('\n'))}</code></pre>`)
  flushPara()
  closeList()
  return html.join('\n')
}

/// 纯文本 → 安全的预格式化 HTML（保留空白）。
export function renderPlainText(src: string): string {
  return `<pre class="doc-plain">${escapeHtml(src.replace(/\r\n?/g, '\n'))}</pre>`
}
