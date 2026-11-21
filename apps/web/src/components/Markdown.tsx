import type { ReactNode } from 'react'
import { t } from '../i18n'
import { type ProjectFileLinks, projectReference } from './chat/projectReference'
import { type BarRow, Bars } from './Bars'

/** The little Markdown CLI answers use: paragraphs, lists, code blocks,
 *  `code`, **bold** and links. Only HTTP and HTTPS may be opened.
 *  Built from React nodes only: nothing from the answer becomes HTML. */
export function Markdown({ text, color, files }: { text: string; color?: string; files?: ProjectFileLinks }) {
  return <div className="md">{blocks(text, color, files)}</div>
}

const MAX_CHART_ROWS = 12

/** A ```raiox-chart block the bot wrote: JSON with a title and rows; anything else stays code. */
function chart(source: string, color: string | undefined, key: number): ReactNode | null {
  try {
    const data: unknown = JSON.parse(source)
    if (typeof data !== 'object' || data === null) return null
    const { title, subtitle, unit, rows } = data as Record<string, unknown>
    if (typeof title !== 'string' || !Array.isArray(rows)) return null
    const clean: BarRow[] = rows
      .filter((r): r is { label: string; value: number } => typeof r?.label === 'string' && typeof r?.value === 'number' && Number.isFinite(r.value))
      .slice(0, MAX_CHART_ROWS)
      .map((r) => ({ label: r.label, value: Math.max(0, r.value) }))
    if (clean.length === 0) return null
    return <Bars key={key} title={title} subtitle={typeof subtitle === 'string' ? subtitle : undefined} unit={typeof unit === 'string' ? unit : ''} rows={clean} color={color} />
  } catch {
    return null
  }
}

const FENCE = /^```/
const ITEM = /^\s*(?:[-*]|\d+[.)])\s+/
const HEADING = /^#{1,6}\s+/

function blocks(text: string, color?: string, files?: ProjectFileLinks): ReactNode[] {
  const out: ReactNode[] = []
  const lines = text.split('\n')
  let i = 0
  while (i < lines.length) {
    const line = lines[i]
    if (FENCE.test(line)) {
      const end = lines.findIndex((l, n) => n > i && FENCE.test(l))
      const stop = end === -1 ? lines.length : end
      const body = lines.slice(i + 1, stop).join('\n')
      const drawn = /^```\s*raiox-chart/.test(line) ? chart(body, color, i) : null
      out.push(drawn ?? <pre key={i}>{body}</pre>)
      i = stop + 1
    } else if (ITEM.test(line)) {
      const start = i
      while (i < lines.length && ITEM.test(lines[i])) i += 1
      const ordered = /^\s*\d/.test(line)
      const items = lines.slice(start, i).map((l, n) => <li key={n}>{inline(l.replace(ITEM, ''), files)}</li>)
      out.push(ordered ? <ol key={start}>{items}</ol> : <ul key={start}>{items}</ul>)
    } else if (line.trim() === '') {
      i += 1
    } else {
      const start = i
      while (i < lines.length && lines[i].trim() !== '' && !FENCE.test(lines[i]) && !ITEM.test(lines[i])) i += 1
      const paragraph = lines.slice(start, i).map((l) => l.replace(HEADING, '')).join('\n')
      out.push(<p key={start}>{inline(paragraph, files)}</p>)
    }
  }
  return out
}

const INLINE = /(`[^`]+`|\*\*[^*]+\*\*|!?\[[^\]]*\]\((?:[^()\r\n]|\([^()\r\n]*\))+\)|<https?:\/\/[^\s<>]+>|https?:\/\/[^\s<>`"']+|(?:[a-z]:)?(?:[\w.-]*[/\\])*[\w.-]+\.[a-z\d]+:\d+(?::\d+)?)/gi
const FILE_POSITION = /^(?:[a-z]:)?(?:[\w.-]*[/\\])*[\w.-]+\.[a-z\d]+:\d+(?::\d+)?$/i

function webUrl(target: string): string | null {
  if (!/^https?:\/\//i.test(target) || /[\s\\\p{Cc}]/u.test(target)) return null
  try {
    const url = new URL(target)
    return url.hostname && !url.username && !url.password ? url.href : null
  } catch {
    return null
  }
}

function externalLink(target: string, label: string, key: number): ReactNode {
  const href = webUrl(target)
  return href ? <a key={key} href={href} target="_blank" rel="noopener noreferrer">{label}</a> : <code key={key}>{label}</code>
}

function fileLink(target: string, label: string, key: number, files?: ProjectFileLinks): ReactNode {
  const file = files && projectReference(target, files.root)
  return file && files ? <button className="md-arquivo" type="button" key={key} onClick={() => files.open(file)}>{label}</button> : null
}

function inline(text: string, files?: ProjectFileLinks): ReactNode[] {
  return text.split(INLINE).map((part, n) => {
    if (part.startsWith('`') && part.endsWith('`') && part.length > 1) {
      const code = part.slice(1, -1)
      return (/(?:[/\\]|:\d+$)/.test(code) && fileLink(code, code, n, files)) || <code key={n}>{code}</code>
    }
    if (part.startsWith('**') && part.endsWith('**') && part.length > 3) return <b key={n}>{inline(part.slice(2, -2), files)}</b>
    const link = part.match(/^!?\[([^\]]*)\]\((.+)\)$/)
    if (link) {
      const [, label, rawTarget] = link
      const target = rawTarget.trim().replace(/^<([^<>]+)>$/, '$1')
      if (/^sandbox:/i.test(target)) return <span key={n}>{label && <>{label} · </>}{t.artifactUnavailable}</span>
      if (!/^https?:\/\//i.test(target)) {
        const local = fileLink(target, label || target, n, files)
        if (local) return local
      }
      return externalLink(target, label || target, n)
    }
    if (/^<https?:\/\//i.test(part)) {
      const target = part.slice(1, -1)
      return externalLink(target, target, n)
    }
    if (/^https?:\/\//i.test(part)) {
      let target = part.replace(/[.,;:!?]+$/, '')
      while (target.endsWith(')') && target.split(')').length > target.split('(').length) target = target.slice(0, -1)
      return <span key={n}>{externalLink(target, target, n)}{part.slice(target.length)}</span>
    }
    if (FILE_POSITION.test(part)) return fileLink(part, part, n, files) ?? part
    return part
  })
}
