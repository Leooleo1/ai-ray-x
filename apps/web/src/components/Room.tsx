import { type FormEvent, type ReactNode, useEffect, useMemo, useRef, useState } from 'react'
import { AGENTS, type AgentId } from '../agents'
import { askAgent, type Report, type RoomMessage } from '../api'
import { suggestionTitle } from '../diet'
import { locale, percent, t } from '../i18n'
import { askLabel, speak } from '../voices'
import { Action } from './Action'
import { Avatar } from './Avatar'
import { CompactionBreakdown } from './Breakdown'
import type { RoomView } from './Sidebar'

const ORDER: AgentId[] = ['raio', 'dieta', 'memo']
const GROUP_MS = 5 * 60_000
/** A beat before the answer, so the reply reads as a reply and not a flash. */
const MIN_TYPING_MS = 800
const MENTION = /@(raio|dieta|memo)\b/i
const OPEN_MENTION = /@(\w*)$/


interface Pending {
  agent: AgentId
  text: string
  since: number
}

interface RoomProps {
  report: Report
  view: RoomView
  onView: (view: RoomView) => void
  onTyping: (agent: AgentId | null) => void
  onChanged: () => void
}

/** Multi-agent chat for this project. Agents speak only when asked: here or via /token-diet. */
export function Room({ report, view, onView, onTyping, onChanged }: RoomProps) {
  const [draft, setDraft] = useState('')
  const [pending, setPending] = useState<Pending | null>(null)
  const [error, setError] = useState<string | null>(null)
  const seen = useRef(report.room.messages.length)
  const end = useRef<HTMLDivElement>(null)
  const project = report.project_dir.split('/').pop() ?? ''
  const all = report.room.messages

  // The optimistic bubble goes away once the server's copy arrives.
  useEffect(() => {
    if (pending && all.some((m) => m.author === 'you' && m.ms >= pending.since - 1000)) {
      setPending(null)
      onTyping(null)
    }
  }, [all, pending, onTyping])
  // Jump to the latest line on open; glide only for lines that arrive while reading.
  const opened = useRef(false)
  useEffect(() => {
    end.current?.scrollIntoView({ block: 'end', behavior: opened.current ? 'smooth' : 'auto' })
    opened.current = true
  }, [all.length, pending, view])
  useEffect(() => {
    const id = window.setTimeout(() => (seen.current = all.length), 600)
    return () => window.clearTimeout(id)
  }, [all.length])

  const messages = useMemo(() => (view === 'channel' ? all : all.filter((m) => belongsTo(m, view))), [all, view])
  const happy = useHappy(all)
  // Approval buttons live only on Dieta's latest answer; older answers stay as text.
  const lastDieta = messages.reduce((found, m, i) => (m.author === 'dieta' && m.kind === 'answer' ? i : found), -1)
  const latest = useMemo(() => {
    const last = new Map<string, number>()
    messages.forEach((m, i) => last.set(m.author, i))
    return last
  }, [messages])
  const mention = draft.match(OPEN_MENTION)
  const mentionOptions = mention ? ORDER.filter((id) => id.startsWith(mention[1].toLowerCase())) : []

  if (!report.room.opted_in) {
    return (
      <main className="chat">
        <section className="cartao sala-fora">
          <h2>{t.roomOffTitle}</h2>
          <p>{t.roomOff}</p>
        </section>
      </main>
    )
  }

  const send = async (agent: AgentId, text: string) => {
    setError(null)
    setPending({ agent, text, since: Date.now() })
    onTyping(agent)
    setDraft('')
    try {
      await Promise.all([askAgent(agent, text), new Promise((resolve) => setTimeout(resolve, MIN_TYPING_MS))])
      onChanged()
    } catch (failure: unknown) {
      setPending(null)
      onTyping(null)
      setError(failure instanceof Error ? failure.message : String(failure))
    }
  }

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const text = draft.trim()
    const target = view === 'channel' ? (text.match(MENTION)?.[1].toLowerCase() as AgentId | undefined) : view
    if (!target) {
      setError(t.mentionHint)
      return
    }
    void send(target, text || askLabel(target))
  }

  const busy = pending !== null
  const quick = view === 'channel' ? ORDER : [view]
  const header = view === 'channel' ? { title: `# ${project}`, hint: t.channelHint } : { title: AGENTS[view].name, hint: AGENTS[view].role }

  return (
    <main className="chat">

      <section className="cartao chat-centro">
        <header className="chat-topo">
          <b>{header.title}</b>
          <small>{header.hint}</small>
          {view !== 'channel' && (
            <button className="voltar" onClick={() => onView('channel')}>
              {t.backToChannel}
            </button>
          )}
          <span className="pilha">
            {(view === 'channel' ? ORDER : [view]).map((id) => (
              <Avatar key={id} agent={AGENTS[id]} size={22} state="still" />
            ))}
          </span>
        </header>

        <div className="chat-mensagens">
          {messages.length === 0 && !pending && <p className="vazio">{t.roomEmpty}</p>}
          {messages.map((message, index) => (
            <Line
              key={`${message.ms}-${index}`}
              message={message}
              previous={messages[index - 1]}
              isNew={view === 'channel' && index >= seen.current}
              live={latest.get(message.author) === index}
              isHappy={happy === message.author && latest.get(message.author) === index}
              extra={index === lastDieta ? <Approvals message={message} report={report} onChanged={onChanged} /> : null}
            />
          ))}
          {pending && (
            <>
              <div className="linha minha nova">
                <span className="balao">{pending.text}</span>
              </div>
              <div className="linha de-agente nova">
                <Avatar agent={AGENTS[pending.agent]} size={32} state="thinking" />
                <div className="corpo">
                  <span className="quem">
                    <b style={{ color: AGENTS[pending.agent].color }}>{AGENTS[pending.agent].name}</b>
                  </span>
                  <span className="digitando" aria-label={t.typingShort}>
                    <i />
                    <i />
                    <i />
                  </span>
                </div>
              </div>
            </>
          )}
          <div ref={end} />
        </div>

        <div className="chat-sugestoes">
          {quick.map((id) => (
            <button key={id} className="chip" disabled={busy} onClick={() => void send(id, askLabel(id))}>
              {askLabel(id)}
            </button>
          ))}
        </div>

        <form className="compositor" onSubmit={submit}>
          {mentionOptions.length > 0 && (
            <ul className="mencoes" role="listbox">
              {mentionOptions.map((id) => (
                <li key={id}>
                  <button type="button" onClick={() => setDraft(draft.replace(OPEN_MENTION, `@${id} `))}>
                    <Avatar agent={AGENTS[id]} size={20} state="still" />
                    <b>@{id}</b>
                    <span>{AGENTS[id].role}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <input
            value={draft}
            maxLength={280}
            onChange={(event) => setDraft(event.target.value)}
            placeholder={view === 'channel' ? t.composeChannel : `${t.composeDirect} ${AGENTS[view].name}`}
            aria-label={t.composeChannel}
            disabled={busy}
          />
          <button className="enviar" type="submit" disabled={busy} aria-label={t.send}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 19V5M5 12l7-7 7 7" />
            </svg>
          </button>
        </form>
        {error && <p className="nota failed">{error}</p>}
      </section>
    </main>
  )
}

function belongsTo(message: RoomMessage, agent: AgentId): boolean {
  if (message.author === 'you') return message.data.agent === agent
  return message.author === agent
}

interface LineProps {
  message: RoomMessage
  previous: RoomMessage | undefined
  isNew: boolean
  live: boolean
  isHappy: boolean
  extra: ReactNode
}

function Line({ message, previous, isNew, live, isHappy, extra }: LineProps) {
  const day = dayLabel(message.ms)
  const showDay = !previous || dayLabel(previous.ms) !== day
  const grouped = !showDay && previous?.author === message.author && message.ms - previous.ms < GROUP_MS
  const time = new Date(message.ms).toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })
  const separator = showDay && (
    <div className="dia">
      <span>{day}</span>
    </div>
  )
  if (message.author === 'you') {
    return (
      <>
        {separator}
        <div className={`linha minha ${isNew ? 'nova' : ''}`}>
          <span className="balao">{speak(message)}</span>
          <span className="hora">{time}</span>
        </div>
      </>
    )
  }
  const agent = AGENTS[message.author]
  return (
    <>
      {separator}
      <div className={`linha de-agente ${grouped ? 'agrupada' : ''} ${isNew ? 'nova' : ''}`}>
        {grouped ? <span className="recuo" /> : <Avatar agent={agent} size={32} state={!live ? 'still' : isHappy ? 'happy' : 'idle'} />}
        <div className="corpo">
          {!grouped && (
            <span className="quem">
              <b style={{ color: agent.color }}>{agent.name}</b>
              <span className="hora">{time}</span>
            </span>
          )}
          <span className="texto">{speak(message)}</span>
          {extra}
        </div>
      </div>
    </>
  )
}

function dayLabel(ms: number): string {
  const date = new Date(ms)
  const today = new Date()
  if (date.toDateString() === today.toDateString()) return t.today
  return date.toLocaleDateString(locale, { weekday: 'long', day: '2-digit', month: 'long' })
}

const HAPPY_MS = 2600

/** The agent that just answered looks happy for a moment. */
function useHappy(all: RoomMessage[]): string | null {
  const [happy, setHappy] = useState<string | null>(null)
  const last = all[all.length - 1]
  useEffect(() => {
    if (!last || last.author === 'you' || Date.now() - last.ms > HAPPY_MS) return undefined
    setHappy(last.author)
    const id = window.setTimeout(() => setHappy(null), HAPPY_MS)
    return () => window.clearTimeout(id)
  }, [last])
  return happy
}

interface ApprovalsProps {
  message: RoomMessage
  report: Report
  onChanged: () => void
}

interface SuggestionData {
  id: string
  title: string
  share: number
  confidence: string
  value?: string | null
}

/** Dieta's applicable suggestions as cards, with the same Apply / Confirm / Undo as the Efficiency tab. */
function Approvals({ message, report, onChanged }: ApprovalsProps) {
  const suggestions = (message.data.suggestions as SuggestionData[] | undefined) ?? []
  const applicable = (message.data.applicable as string[] | undefined) ?? []
  const claude = report.sources.find((source) => source.overview.source === 'claude')
  // The buttons act on Dieta's suggestion as computed now, never on numbers saved in an old message.
  const items = suggestions
    .filter((s) => applicable.includes(s.id))
    .flatMap((s) => {
      const now = claude?.suggestions.find((current) => current.id === s.id)
      return now ? [{ id: now.id, title: now.title, share: now.saving_share, confidence: now.confidence, value: now.value }] : []
    })
  if (items.length === 0) return null
  return (
    <div className="aprovacoes">
      {items.map((s) => {
        const change = report.changes.find((c) => c.suggestion === s.id && c.undone_ms === null)
        return (
          <div key={s.id} className="aprovacao">
            <div className="aprovacao-topo">
              <b>{suggestionTitle(s.id, s.title)}</b>
              <span className="num">
                ~
                {percent(s.share)}
              </span>
            </div>
            {s.id === 'compact-earlier' && claude && <CompactionBreakdown model={claude.efficiency.compaction} />}
            <Action
              id={s.id}
              value={s.value}
              source="claude"
              change={change}
              outcome={report.outcomes.find((o) => o.change_id === change?.id)}
              onChanged={onChanged}
            />
          </div>
        )
      })}
    </div>
  )
}
