import { useState } from 'react'
import type { Bot, BotMessage } from '../../api'
import { faceForBot } from '../../avatars'
import type { Bots } from '../../bots'
import { exchanges } from '../../exchanges'
import { locale, t } from '../../i18n'
import { Avatar } from '../Avatar'
import type { ChatNav } from './nav'

const DAY_MS = 86_400_000
const PERIODS = [
  { id: 'today', days: 1 },
  { id: '7', days: 7 },
  { id: '30', days: 30 },
  { id: 'all', days: Infinity },
] as const
const MAX_HITS = 30
/** Searching messages starts at this many characters. */
const MIN_QUERY = 3

/** The last thing said, as one line for the list. */
function preview(m: BotMessage | undefined): string {
  if (!m) return t.noMessagesYet
  const text = m.data.reply?.text ?? m.data.text ?? m.data.proposal ?? m.data.error ?? ''
  return text.replace(/[`*#>]/g, '').split('\n').find((l) => l.trim()) ?? ''
}

function when(ms: number | undefined): string {
  if (!ms) return ''
  const date = new Date(ms)
  if (date.toDateString() === new Date().toDateString()) return date.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })
  return date.toLocaleDateString(locale, { day: 'numeric', month: 'short' })
}

interface BotListProps {
  bots: Bots
  current: string
  unread: Set<string>
  nav: ChatNav
}

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

/** Every bot as a conversation: face, name, when and the last line, newest first, with search. */
export function BotList({ bots, current, unread, nav }: BotListProps) {
  const pairs = exchanges(bots.state.bots)
  const [query, setQuery] = useState('')
  const [period, setPeriod] = useState<(typeof PERIODS)[number]['id']>('all')
  const needle = query.trim().toLowerCase()
  const rows = [...bots.state.bots]
    .filter((b: Bot) => !needle || `${b.name} ${b.persona?.name ?? ''} ${b.agent}`.toLowerCase().includes(needle))
    .sort((a, b) => (b.messages.at(-1)?.ms ?? 0) - (a.messages.at(-1)?.ms ?? 0))
  return (
    <nav className="conversas" aria-label={t.bots}>
      <div className="conversas-topo">
        <input value={query} placeholder={t.search2} aria-label={t.search2} onChange={(event) => setQuery(event.target.value)} />
        <button type="button" aria-label={t.addBot} title={t.addBot} onClick={nav.add}>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 5v14M5 12h14" />
          </svg>
        </button>
      </div>
      {!needle && (
        <button type="button" className={`conversas-acao ${current === 'grupo' ? 'ativo' : ''}`} onClick={nav.askMany}>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M4 12h6M14 6l6-2M14 12h6M14 18l6 2M10 12l4-6M10 12l4 6" />
          </svg>
          {t.askMany}
        </button>
      )}
      {needle.length >= MIN_QUERY && <SearchHits bots={bots.state.bots} needle={needle} period={period} onPeriod={setPeriod} onOpen={nav.openMessage} />}
      {bots.state.batches.length > 0 && !needle && (
        <>
          <span className="conversas-grupo">{t.groups}</span>
          <ul className="conversas-trocas">
            {bots.state.batches.map((batch) => (
              <li key={batch.id}>
                <button type="button" className={current === `grupo:${batch.id}` ? 'ativo' : ''} onClick={() => nav.openBatch(batch.id)}>
                  <span className="troca-rostos">
                    {batch.items.slice(0, 2).map((item) => {
                      const bot = bots.state.bots.find((b) => b.id === item.bot)
                      return bot ? <Avatar key={item.bot} agent={faceForBot(bot)} size={24} state="still" /> : null
                    })}
                  </span>
                  <span className="conversa-texto">
                    <span className="conversa-linha">
                      <b>{batch.text}</b>
                      <small>{when(batch.created_ms)}</small>
                    </span>
                    <span className="conversa-previa">{`${batch.items.length} bots`}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
      {pairs.length > 0 && !needle && (
        <>
          <span className="conversas-grupo">{t.exchanges}</span>
          <ul className="conversas-trocas">
            {pairs.map((pair) => {
              const [a, b] = [pair.a, pair.b].map((name) => bots.state.bots.find((bot) => bot.name === name))
              const key = `${pair.a}|${pair.b}`
              return (
                <li key={key}>
                  <button type="button" className={current === key ? 'ativo' : ''} onClick={() => nav.openExchange(pair.a, pair.b)}>
                    <span className="troca-rostos">
                      {a && <Avatar agent={faceForBot(a)} size={24} state="still" />}
                      {b && <Avatar agent={faceForBot(b)} size={24} state="still" />}
                    </span>
                    <span className="conversa-texto">
                      <span className="conversa-linha">
                        <b>{`${pair.a} ⇄ ${pair.b}`}</b>
                        <small>{when(pair.lastMs)}</small>
                      </span>
                      <span className="conversa-previa">{pair.waiting > 0 ? `${pair.waiting} ${t.waitingToHandle}` : `${pair.items.length} ${t.steps2}`}</span>
                    </span>
                  </button>
                </li>
              )
            })}
          </ul>
          <span className="conversas-grupo">{t.projects}</span>
        </>
      )}
      <ul>
        {rows.map((bot) => {
          const last = bot.messages.at(-1)
          return (
            <li key={bot.id}>
              <button type="button" className={`${bot.id === current ? 'ativo' : ''} ${unread.has(bot.id) ? 'nao-lido' : ''}`} onClick={() => nav.openBot(bot.id)}>
                <Avatar agent={faceForBot(bot)} size={36} state={bot.busy ? 'thinking' : 'still'} />
                <span className="conversa-texto">
                  <span className="conversa-linha">
                    <b>{bot.persona ? `${bot.name} · ${bot.persona.name}` : bot.name}</b>
                    <small>{when(last?.ms)}</small>
                  </span>
                  <span className="conversa-previa">
                    {bot.busy ? t.typingShort : preview(last)}
                    {unread.has(bot.id) && <i className="nao-lida" aria-label={t.unreadAnswer} />}
                  </span>
                </span>
              </button>
            </li>
          )
        })}
      </ul>
    </nav>
  )
}

interface SearchHitsProps {
  bots: Bot[]
  needle: string
  period: (typeof PERIODS)[number]['id']
  onPeriod: (period: (typeof PERIODS)[number]['id']) => void
  onOpen: (bot: string, thread: string, ms: number) => void
}

/** Messages found, with the period as one grouped control. */
function SearchHits({ bots, needle, period, onPeriod, onOpen }: SearchHitsProps) {
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
