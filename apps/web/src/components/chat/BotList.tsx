import { useState } from 'react'
import type { Bot, BotMessage } from '../../api'
import { faceForBot } from '../../avatars'
import type { Bots } from '../../bots'
import { exchanges } from '../../exchanges'
import { locale, t } from '../../i18n'
import { Avatar } from '../Avatar'

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
  onOpen: (id: string) => void
  onOpenExchange: (a: string, b: string) => void
  onAdd: () => void
}

/** Every bot as a conversation: face, name, when and the last line, newest first, with search. */
export function BotList({ bots, current, unread, onOpen, onOpenExchange, onAdd }: BotListProps) {
  const pairs = exchanges(bots.state.bots)
  const [query, setQuery] = useState('')
  const needle = query.trim().toLowerCase()
  const rows = [...bots.state.bots]
    .filter((b: Bot) => !needle || `${b.name} ${b.persona?.name ?? ''} ${b.agent}`.toLowerCase().includes(needle))
    .sort((a, b) => (b.messages.at(-1)?.ms ?? 0) - (a.messages.at(-1)?.ms ?? 0))
  return (
    <nav className="conversas" aria-label={t.bots}>
      <div className="conversas-topo">
        <input value={query} placeholder={t.search2} aria-label={t.search2} onChange={(event) => setQuery(event.target.value)} />
        <button type="button" aria-label={t.addBot} title={t.addBot} onClick={onAdd}>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M12 5v14M5 12h14" />
          </svg>
        </button>
      </div>
      {pairs.length > 0 && !needle && (
        <>
          <span className="conversas-grupo">{t.exchanges}</span>
          <ul className="conversas-trocas">
            {pairs.map((pair) => {
              const [a, b] = [pair.a, pair.b].map((name) => bots.state.bots.find((bot) => bot.name === name))
              const key = `${pair.a}|${pair.b}`
              return (
                <li key={key}>
                  <button type="button" className={current === key ? 'ativo' : ''} onClick={() => onOpenExchange(pair.a, pair.b)}>
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
              <button type="button" className={`${bot.id === current ? 'ativo' : ''} ${unread.has(bot.id) ? 'nao-lido' : ''}`} onClick={() => onOpen(bot.id)}>
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
