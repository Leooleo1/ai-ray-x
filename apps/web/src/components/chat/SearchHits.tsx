import { useState } from 'react'
import type { Bot } from '../../api'
import { t } from '../../i18n'
import { when } from './listParts'

const DAY_MS = 86_400_000
const PERIODS = [
  { id: 'today', days: 1 },
  { id: '7', days: 7 },
  { id: '30', days: 30 },
  { id: 'all', days: Infinity },
] as const
type Period = (typeof PERIODS)[number]['id']
const MAX_HITS = 30
/** Searching messages starts at this many characters. */
export const MIN_QUERY = 3

interface Hit {
  bot: Bot
  thread: string
  where: string
  ms: number
  text: string
}

/** Messages in every bot's conversations that contain the text, newest first. */
function search(bots: Bot[], needle: string, sinceMs: number): Hit[] {
  const hits: Hit[] = []
  for (const bot of bots) {
    const conversations = [{ id: 'main', title: bot.name, messages: bot.messages }, ...bot.threads]
    for (const c of conversations) {
      for (const m of c.messages) {
        const text = m.data.reply?.text ?? m.data.body ?? m.data.text ?? ''
        if (m.ms >= sinceMs && text.toLowerCase().includes(needle)) hits.push({ bot, thread: c.id, where: c.id === 'main' ? bot.name : `${bot.name} · ${c.title}`, ms: m.ms, text })
      }
    }
  }
  return hits.sort((a, b) => b.ms - a.ms).slice(0, MAX_HITS)
}

/** A line of text around the match. */
function around(text: string, needle: string): string {
  const flat = text.replace(/[`*#>]/g, '').replace(/\s+/g, ' ')
  const at = flat.toLowerCase().indexOf(needle)
  const start = Math.max(0, at - 30)
  return `${start > 0 ? '…' : ''}${flat.slice(start, start + 110)}`
}

interface SearchHitsProps {
  bots: Bot[]
  needle: string
  onOpen: (bot: string, thread: string, ms: number) => void
}

/** Messages found in every project's conversations, with the period as one grouped control. */
export function SearchHits({ bots, needle, onOpen }: SearchHitsProps) {
  const [period, onPeriod] = useState<Period>('all')
  const days = PERIODS.find((p) => p.id === period)?.days ?? Infinity
  const since = days === Infinity ? 0 : days === 1 ? new Date(new Date().toDateString()).getTime() : Date.now() - days * DAY_MS
  const hits = search(bots, needle, since)
  const labels: Record<string, string> = { today: t.periodToday, '7': t.period7, '30': t.period30, all: t.periodAll }
  return (
    <>
      <span className="conversas-grupo">{t.messagesFound}</span>
      <div className="aparencia-segmentos busca-periodo" role="radiogroup" aria-label={t.messagesFound}>
        {PERIODS.map((p) => (
          <button key={p.id} type="button" role="radio" aria-checked={period === p.id} className={period === p.id ? 'ativo' : ''} onClick={() => onPeriod(p.id)}>
            {labels[p.id]}
          </button>
        ))}
      </div>
      {hits.length === 0 && <p className="conversas-vazio">{t.noMatches}</p>}
      <ul className="conversas-achados">
        {hits.map((hit) => (
          <li key={`${hit.bot.id}-${hit.thread}-${hit.ms}`}>
            <button type="button" onClick={() => onOpen(hit.bot.id, hit.thread, hit.ms)}>
              <span className="conversa-linha">
                <b>{hit.where}</b>
                <small>{when(hit.ms)}</small>
              </span>
              <span className="conversa-previa">{around(hit.text, needle)}</span>
            </button>
          </li>
        ))}
      </ul>
    </>
  )
}
