import type { ReactNode } from 'react'
import { type BarRow, Bars } from './Bars'

/** The little Markdown CLI answers use: paragraphs, lists, code blocks,
 *  `code`, **bold** and [label](target) links shown as their label.
 *  Built from React nodes only: nothing from the answer becomes HTML. */
export function Markdown({ text, color }: { text: string; color?: string }) {
  return <div className="md">{blocks(text, color)}</div>
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

function blocks(text: string, color?: string): ReactNode[] {
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
      const items = lines.slice(start, i).map((l, n) => <li key={n}>{inline(l.replace(ITEM, ''))}</li>)
      out.push(ordered ? <ol key={start}>{items}</ol> : <ul key={start}>{items}</ul>)
    } else if (line.trim() === '') {
      i += 1
    } else {
      const start = i
      while (i < lines.length && lines[i].trim() !== '' && !FENCE.test(lines[i]) && !ITEM.test(lines[i])) i += 1
      const paragraph = lines.slice(start, i).map((l) => l.replace(HEADING, '')).join('\n')
      out.push(<p key={start}>{inline(paragraph)}</p>)
    }
  }
  return out
}

const INLINE = /(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\([^)]+\))/g

function inline(text: string): ReactNode[] {
  return text.split(INLINE).map((part, n) => {
    if (part.startsWith('`') && part.endsWith('`') && part.length > 1) return <code key={n}>{part.slice(1, -1)}</code>
    if (part.startsWith('**') && part.endsWith('**') && part.length > 3) return <b key={n}>{part.slice(2, -2)}</b>
    const link = part.match(/^\[([^\]]+)\]\([^)]+\)$/)
    if (link) return <code key={n}>{link[1]}</code>
    return part
  })
}
