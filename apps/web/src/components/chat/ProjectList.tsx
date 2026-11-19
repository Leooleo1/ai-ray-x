import { useState } from 'react'
import type { Bot, BotMessage } from '../../api'
import { AGENT_LABEL, faceForBot } from '../../avatars'
import type { Bots } from '../../bots'
import { exchanges } from '../../exchanges'
import { t } from '../../i18n'
import { Avatar } from '../Avatar'
import { count, preview, when } from './listParts'
import type { ChatNav } from './nav'
import { MIN_QUERY, SearchHits } from './SearchHits'

const MAIN = 'main'
/** Order the IAs of a project the same way everywhere. */
const AGENT_ORDER = ['claude', 'codex', 'cursor']

interface ProjectListProps {
  bots: Bots
  /** The project whose conversations are listed. */
  project: string
  /** `<bot>:<conversation>`, `troca:<a>|<b>` or `grupo:<id>`. */
  current: string
  unread: Set<string>
  nav: ChatNav
  /** Opens a conversation of one of the project's IAs. */
  onThread: (bot: string, thread: string) => void
  /** Starts a new conversation with the IA open now; absent where there is none. */
  onNew?: () => void
  onClose: (bot: string, thread: string) => void
  /** On a phone the list is a sheet over the chat. */
  open: boolean
}

interface Row {
  bot: Bot
  thread: string
  title: string
  last: BotMessage | undefined
}

function rowsOf(bot: Bot): Row[] {
  const main: Row = { bot, thread: MAIN, title: t.mainConversation, last: bot.messages.at(-1) }
  const side = bot.threads.map((x) => ({ bot, thread: x.id, title: x.title, last: x.messages.at(-1) }))
  // The main conversation first, then the others with the newest on top.
  return [main, ...side.sort((a, b) => (b.last?.ms ?? 0) - (a.last?.ms ?? 0))]
}

/** One project's conversations: each IA's main one and its parallel ones, then its exchanges and group questions. */
export function ProjectList({ bots, project, current, unread, nav, onThread, onNew, onClose, open }: ProjectListProps) {
  const [query, setQuery] = useState('')
  const needle = query.trim().toLowerCase()
  const team = bots.state.bots.filter((b) => b.name === project).sort((a, b) => AGENT_ORDER.indexOf(a.agent) - AGENT_ORDER.indexOf(b.agent))
  const ids = new Set(team.map((b) => b.id))
  const pairs = exchanges(bots.state.bots).filter((x) => x.a === project || x.b === project)
  const groups = bots.state.batches.filter((g) => g.items.some((item) => ids.has(item.bot)))
  const lead = team[0]
  return (
    <nav className={`conversas ${open ? 'aberta' : ''}`} aria-label={t.conversationsOf.replace('{p}', project)}>
      <header className="projeto-topo">
        {lead && <Avatar agent={faceForBot(lead)} size={28} state="still" />}
        <span>
          <b>{project}</b>
          <small>{team.map((b) => AGENT_LABEL[b.agent]).join(' · ')}</small>
        </span>
      </header>
      <div className="conversas-topo">
        <input value={query} placeholder={t.searchAll} aria-label={t.searchAll} onChange={(event) => setQuery(event.target.value)} />
        {onNew && (
          <button type="button" aria-label={t.newThread} title={t.newThread} onClick={onNew}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 5v14M5 12h14" />
            </svg>
          </button>
        )}
      </div>
      {needle.length >= MIN_QUERY ? (
        <SearchHits bots={bots.state.bots} needle={needle} onOpen={nav.openMessage} />
      ) : (
        <>
          {team.map((bot) => (
            <section key={bot.id} className="conversas-ia">
              {team.length > 1 && <span className="conversas-grupo">{[AGENT_LABEL[bot.agent], bot.persona?.name].filter(Boolean).join(' · ')}</span>}
              <ul>
                {rowsOf(bot).map((row) => {
                  const key = `${bot.id}:${row.thread}`
                  const fresh = row.thread === MAIN && unread.has(bot.id)
                  const busy = bot.busy && (bot.busy_thread ?? MAIN) === row.thread
                  return (
                    <li key={key} className="conversa-item">
                      <button type="button" className={`${current === key ? 'ativo' : ''} ${fresh ? 'nao-lido' : ''}`} onClick={() => onThread(bot.id, row.thread)}>
                        <span className="conversa-texto">
                          <span className="conversa-linha">
                            <b>{row.title}</b>
                            <small>{when(row.last?.ms)}</small>
                          </span>
                          <span className="conversa-previa">
                            {busy ? t.typingShort : preview(row.last)}
                            {fresh && <i className="nao-lida" aria-label={t.unreadAnswer} />}
                          </span>
                        </span>
                      </button>
                      {row.thread !== MAIN && (
                        <button type="button" className="fechar-conversa" aria-label={t.closeThread} title={t.closeThread} disabled={busy} onClick={() => onClose(bot.id, row.thread)}>
                          <svg viewBox="0 0 24 24" aria-hidden="true">
                            <path d="M7 7l10 10M17 7L7 17" />
                          </svg>
                        </button>
                      )}
                    </li>
                  )
                })}
              </ul>
            </section>
          ))}
          {pairs.length > 0 && (
            <section className="conversas-ia">
              <span className="conversas-grupo">{t.exchanges}</span>
              <ul>
                {pairs.map((pair) => {
                  const other = pair.a === project ? pair.b : pair.a
                  const key = `troca:${pair.a}|${pair.b}`
                  return (
                    <li key={key} className="conversa-item">
                      <button type="button" className={current === key ? 'ativo' : ''} onClick={() => nav.openExchange(pair.a, pair.b)}>
                        <span className="conversa-texto">
                          <span className="conversa-linha">
                            <b>{`⇄ ${other}`}</b>
                            <small>{when(pair.lastMs)}</small>
                          </span>
                          <span className="conversa-previa">{pair.waiting > 0 ? `${pair.waiting} ${t.waitingToHandle}` : `${pair.items.length} ${t.steps2}`}</span>
                        </span>
                      </button>
                    </li>
                  )
                })}
              </ul>
            </section>
          )}
          {groups.length > 0 && (
            <section className="conversas-ia">
              <span className="conversas-grupo">{t.groups}</span>
              <ul>
                {groups.map((group) => (
                  <li key={group.id} className="conversa-item">
                    <button type="button" className={current === `grupo:${group.id}` ? 'ativo' : ''} onClick={() => nav.openBatch(group.id)}>
                      <span className="conversa-texto">
                        <span className="conversa-linha">
                          <b>{group.text}</b>
                          <small>{when(group.created_ms)}</small>
                        </span>
                        <span className="conversa-previa">{count(group.items.length, t.aisCount)}</span>
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </>
      )}
    </nav>
  )
}
