import { useState } from 'react'
import { type Bot, type BotMessage, busyIn } from '../../api'
import { AGENT_LABEL, faceForBot, personaName } from '../../avatars'
import type { Bots } from '../../bots'
import { exchanges } from '../../exchanges'
import { t } from '../../i18n'
import { AgentLogo } from '../AgentLogo'
import { Avatar } from '../Avatar'
import { count, preview, when } from './listParts'
import type { ChatNav } from './nav'
import { MIN_QUERY, SearchHits } from './SearchHits'

const MAIN = 'main'
/** Order the IAs of a project the same way everywhere. */
const AGENT_ORDER = ['claude', 'codex', 'cursor', 'antigravity', 'opencode']

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
  task?: boolean
}

function rowsOf(bot: Bot): Row[] {
  const main: Row = { bot, thread: MAIN, title: t.mainConversation, last: bot.messages.at(-1) }
  const side = bot.threads.map((x) => ({ bot, thread: x.id, title: x.title, last: x.messages.at(-1), task: x.task }))
  // The main conversation first, then the others with the newest on top.
  return [main, ...side.sort((a, b) => (b.last?.ms ?? 0) - (a.last?.ms ?? 0))]
}

/** One project's conversations: each IA's main one and its parallel ones, then its exchanges and group questions. */
export function ProjectList({ bots, project, current, unread, nav, onThread, onNew, onClose, open }: ProjectListProps) {
  const [query, setQuery] = useState('')
  const [closing, setClosing] = useState<string | null>(null)
  const needle = query.trim().toLowerCase()
  const team = bots.state.bots.filter((b) => b.name === project).sort((a, b) => AGENT_ORDER.indexOf(a.agent) - AGENT_ORDER.indexOf(b.agent))
  const ids = new Set(team.map((b) => b.id))
  const pairs = exchanges(bots.state.bots).filter((x) => x.a === project || x.b === project)
  const groups = bots.state.batches.filter((g) => g.items.some((item) => ids.has(item.bot)))
  const lead = team[0]
  return (
    <nav className={`conversas ${open ? 'aberta' : ''}`} aria-label={t.conversationsOf.replace('{p}', project)}>
      <header className="projeto-topo">
        <button type="button" className="projeto-voltar" aria-label={t.backToConversations} title={t.backToConversations} onClick={nav.add}>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M15 5l-7 7 7 7" />
          </svg>
        </button>
        {lead && <Avatar agent={faceForBot(lead)} size={28} state="still" />}
        <span>
          <b>{project}</b>
          <small>{team.map((b) => AGENT_LABEL[b.agent]).join(' · ')}</small>
        </span>
      </header>
      <div className="conversas-topo">
        <div className="busca-wrap">
          <input value={query} placeholder={t.searchAll} aria-label={t.searchAll} onChange={(event) => setQuery(event.target.value)} />
          {query && (
            <button type="button" className="campo-limpar" aria-label={t.clearSearch} title={t.clearSearch} onClick={() => setQuery('')}>
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>
        {onNew && (
          <button type="button" aria-label={t.newThread} title={t.newThread} onClick={onNew}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 5v14M5 12h14" />
            </svg>
          </button>
        )}
      </div>
      {needle.length > 0 && needle.length < MIN_QUERY && <p className="conversas-vazio">{t.searchHint.replace('{n}', String(MIN_QUERY))}</p>}
      {needle.length >= MIN_QUERY ? (
        <SearchHits bots={bots.state.bots} needle={needle} onOpen={nav.openMessage} />
      ) : (
        <div className="conversas-rolagem">
          {team.map((bot) => (
            <section key={bot.id} className="conversas-ia">
              {team.length > 1 && (
                <span className="conversas-grupo">
                  <AgentLogo agent={bot.agent} size={12} />
                  {[AGENT_LABEL[bot.agent], personaName(bot)].filter(Boolean).join(' · ')}
                </span>
              )}
              <ul>
                {rowsOf(bot).map((row) => {
                  const key = `${bot.id}:${row.thread}`
                  const fresh = row.thread === MAIN && unread.has(bot.id)
                  const busy = Boolean(busyIn(bot, row.thread))
                  const confirming = closing === key
                  return (
                    <li key={key} className="conversa-item">
                      <button type="button" className={`${current === key ? 'ativo' : ''} ${fresh ? 'nao-lido' : ''}`} onClick={() => onThread(bot.id, row.thread)}>
                        <span className="conversa-rosto rosto-com-ia">
                          <Avatar agent={faceForBot(bot)} size={30} state={busy ? 'thinking' : 'still'} />
                          <span className="ia-selo">
                            <AgentLogo agent={bot.agent} size={11} />
                          </span>
                        </span>
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
                      {row.thread !== MAIN && !row.task && (
                        <button
                          type="button"
                          className={`fechar-conversa ${confirming ? 'confirmar' : ''}`}
                          aria-label={confirming ? t.closeThreadSure : t.closeThread}
                          title={confirming ? t.closeThreadSure : t.closeThread}
                          disabled={busy}
                          onBlur={() => setClosing(null)}
                          onClick={() => {
                            if (!confirming) setClosing(key)
                            else {
                              setClosing(null)
                              onClose(bot.id, row.thread)
                            }
                          }}
                        >
                          {confirming ? (
                            t.closeThreadSure
                          ) : (
                            <svg viewBox="0 0 24 24" aria-hidden="true">
                              <path d="M7 7l10 10M17 7L7 17" />
                            </svg>
                          )}
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
                  const face = bots.state.bots.find((b) => b.name === other)
                  return (
                    <li key={key} className="conversa-item">
                      <button type="button" className={current === key ? 'ativo' : ''} onClick={() => nav.openExchange(pair.a, pair.b)}>
                        <span className="conversa-rosto">{face && <Avatar agent={faceForBot(face)} size={30} state="still" />}</span>
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
                      <span className="conversa-rosto grupo" aria-hidden="true">
                        <svg viewBox="0 0 24 24">
                          <path d="M4 12h5M9 12l4-6 7-2M9 12h11M9 12l4 6 7 2" />
                        </svg>
                      </span>
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
        </div>
      )}
    </nav>
  )
}
