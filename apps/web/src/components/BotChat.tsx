import { type FormEvent, useEffect, useRef, useState } from 'react'
import type { Bot, BotMessage } from '../api'
import { AGENT_LABEL, BOT_FACES, type Bots } from '../bots'
import { compact, locale, t } from '../i18n'
import { Avatar } from './Avatar'
import { Markdown } from './Markdown'

const TICK_MS = 1000

interface BotChatProps {
  bot: Bot
  bots: Bots
}

/** A conversation with the person's own CLI in one project: read-only talk, edits after approval. */
export function BotChat({ bot, bots }: BotChatProps) {
  const [draft, setDraft] = useState('')
  const end = useRef<HTMLDivElement>(null)
  const messages = bot.messages
  const label = AGENT_LABEL[bot.agent]
  const pending = pendingProposal(messages)

  useEffect(() => {
    end.current?.scrollIntoView({ block: 'end' })
  }, [messages.length, bot.busy, bot.id])

  const send = async (text: string) => {
    const clean = text.trim()
    if (!clean || bot.busy) return
    if (await bots.act('ask', { id: bot.id, text: clean })) setDraft('')
  }
  const submit = (event: FormEvent) => {
    event.preventDefault()
    void send(draft)
  }

  return (
    <main className="chat">
      <section className="cartao chat-centro">
        <header className="chat-topo">
          <b>{bot.name}</b>
          <small>
            {label} · {t.botTalkHint}
          </small>
          <button className="voltar" disabled={bot.busy || !bot.has_session} onClick={() => void bots.act('reset', { id: bot.id })}>
            {t.newConversation}
          </button>
          <span className="pilha">
            <Avatar agent={BOT_FACES[bot.agent]} size={22} state="still" />
          </span>
        </header>

        <div className="chat-mensagens">
          {messages.map((message, index) => (
            <Line
              key={`${message.ms}-${index}`}
              bot={bot}
              message={message}
              isLatest={index === messages.length - 1}
              approvable={index === pending}
              onApprove={() => void bots.act('approve', { id: bot.id })}
            />
          ))}
          {bot.busy && <Waiting bot={bot} />}
          <div ref={end} />
        </div>

        {messages.length === 0 && !bot.busy && (
          <div className="chat-sugestoes">
            {t.botStarters.map((starter) => (
              <button key={starter} className="chip" onClick={() => void send(starter)}>
                {starter}
              </button>
            ))}
          </div>
        )}

        <form className="compositor" onSubmit={submit}>
          <input
            value={draft}
            maxLength={bots.state.max_text}
            onChange={(event) => setDraft(event.target.value)}
            placeholder={`${t.composeBot} ${label}`}
            aria-label={`${t.composeBot} ${label}`}
            disabled={bot.busy}
          />
          <button className="enviar" type="submit" disabled={bot.busy || !draft.trim()} aria-label={t.send}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 19V5M5 12l7-7 7 7" />
            </svg>
          </button>
        </form>
        {bots.error && <p className="nota failed chat-erro">{bots.error}</p>}
      </section>
    </main>
  )
}

/** The answer whose proposal can still be approved: the latest one, if nothing but failures came after it. */
function pendingProposal(messages: BotMessage[]): number {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    const m = messages[i]
    if (m.kind === 'failed') continue
    if (m.kind === 'approve') return -1
    return m.kind === 'answer' && m.data.reply?.proposal ? i : -1
  }
  return -1
}

const clock = (ms: number) => new Date(ms).toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })

interface LineProps {
  bot: Bot
  message: BotMessage
  isLatest: boolean
  approvable: boolean
  onApprove: () => void
}

function Line({ bot, message, isLatest, approvable, onApprove }: LineProps) {
  const face = BOT_FACES[bot.agent]
  if (message.kind === 'reset') {
    return (
      <div className="dia">
        <span>{t.conversationReset}</span>
      </div>
    )
  }
  if (message.author === 'you' && message.kind === 'approve') {
    return <p className="linha-sistema">{`${t.approved}: ${(message.data.proposal ?? '').replace(/`/g, '')}`}</p>
  }
  if (message.author === 'you') {
    return (
      <div className="linha minha">
        <span className="balao">{message.data.text}</span>
        <span className="hora">{clock(message.ms)}</span>
      </div>
    )
  }
  const reply = message.data.reply
  return (
    <div className="linha de-agente">
      <Avatar agent={face} size={32} state={isLatest ? 'idle' : 'still'} />
      <div className="corpo">
        <span className="quem">
          <b style={{ color: face.color }}>{AGENT_LABEL[bot.agent]}</b>
          <span className="hora">{clock(message.ms)}</span>
          {message.data.mode === 'edit' && <span className="editou">{t.editedBadge}</span>}
        </span>
        {message.kind === 'failed' && <p className="nota failed quebra">{message.data.error}</p>}
        {reply && <Markdown text={reply.text} />}
        {reply && (
          <span className="bot-meta">
            {`${t.readWord} ${compact(reply.read_tokens)} tokens · ${Math.max(1, Math.round(reply.duration_ms / 1000))} s`}
          </span>
        )}
        {reply?.proposal && (
          <div className="proposta">
            <span className="proposta-rotulo">{t.proposal}</span>
            <Markdown text={reply.proposal} />
            {approvable && (
              <span className="acao">
                <button className="cheio" disabled={bot.busy} onClick={onApprove}>
                  {t.approveEdit}
                </button>
                <span className="nota">{bot.agent === 'claude' ? t.claudeEditScope : t.codexEditScope}</span>
              </span>
            )}
          </div>
        )}
      </div>
    </div>
  )
}

/** The bot is running: what it is doing and for how long. */
function Waiting({ bot }: { bot: Bot }) {
  const last = [...bot.messages].reverse().find((m) => m.author === 'you')
  const since = last?.ms ?? Date.now()
  const [now, setNow] = useState(Date.now())
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), TICK_MS)
    return () => window.clearInterval(timer)
  }, [])
  const doing = last?.kind === 'approve' ? t.botEditing : t.botReading
  const face = BOT_FACES[bot.agent]
  return (
    <div className="linha de-agente nova">
      <Avatar agent={face} size={32} state="thinking" />
      <div className="corpo">
        <span className="quem">
          <b style={{ color: face.color }}>{AGENT_LABEL[bot.agent]}</b>
        </span>
        <span className="espera">
          <span className="digitando" aria-hidden="true">
            <i />
            <i />
            <i />
          </span>
          {`${doing} · ${Math.max(0, Math.round((now - since) / 1000))} s`}
        </span>
      </div>
    </div>
  )
}
