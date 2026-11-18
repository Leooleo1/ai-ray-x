import { useEffect, useRef, useState } from 'react'
import { type Bot, type BotMessage, type Inbox, botInbox } from '../api'
import { AGENT_LABEL, faceForBot, speakerOf } from '../avatars'
import type { Bots } from '../bots'
import { t } from '../i18n'
import { Avatar } from './Avatar'
import { BotSettings } from './BotSettings'
import { ActivityBar } from './chat/ActivityBar'
import { BotLine } from './chat/BotLine'
import { BotList } from './chat/BotList'
import { ModelPicker } from './chat/ModelPicker'
import { Composer } from './chat/Composer'
import { Expandable } from './chat/Expandable'
import { useFollowBottom } from './chat/useFollowBottom'
import './chat/chat.css'

/** The project's ai-memory inbox is asked again this often while the chat is open. */
const INBOX_MS = 30_000
const INBOX_ID = 'caixa-ai-memory'
const PROPOSAL_ID = 'esperando-aprovacao'

interface BotChatProps {
  bot: Bot
  bots: Bots
  onOpen: (id: string) => void
  onAdd: () => void
}

/** A conversation with the person's own CLI in one project: read-only talk, edits after approval. */
export function BotChat({ bot, bots, onOpen, onAdd }: BotChatProps) {
  const [settingsOpen, setSettingsOpen] = useState(false)
  const inbox = useInbox(bot)
  const list = useRef<HTMLDivElement>(null)
  const messages = bot.messages
  const mail = inbox.value?.messages ?? []
  const { unseen, jump } = useFollowBottom(list, messages.length + mail.length + (bot.busy ? 1 : 0), bot.id)
  const face = faceForBot(bot)
  const speaker = speakerOf(bot)
  const pending = pendingProposal(messages)
  const sent = new Set(messages.filter((m) => m.kind === 'sent').map((m) => `${m.data.answer_ms}|${m.data.project}`))
  const lastAsk = [...messages].reverse().find((m) => m.author === 'you')
  const waiting = (pending >= 0 ? 1 : 0) + mail.length

  const send = async (text: string) => (await bots.act('ask', { id: bot.id, text })) !== null
  const see = () => document.getElementById(pending >= 0 ? PROPOSAL_ID : INBOX_ID)?.scrollIntoView({ behavior: 'smooth', block: 'center' })

  return (
    <main className="chat">
      <section className="cartao chat-centro bot-chat">
        <BotList bots={bots} current={bot.id} onOpen={onOpen} onAdd={onAdd} />
        <div className="conversa">
        <header className="chat-topo bot-topo">
          <span className="bot-aba">
            <Avatar agent={face} size={22} state={bot.busy ? 'thinking' : 'still'} />
            <b>{bot.persona ? `${bot.name} · ${bot.persona.name}` : bot.name}</b>
          </span>
          <button type="button" className="icone-btn" title={t.newConversation} aria-label={t.newConversation} disabled={bot.busy || !bot.has_session} onClick={() => void bots.act('reset', { id: bot.id })}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M12 5v14M5 12h14" />
            </svg>
          </button>
          <ModelPicker bot={bot} bots={bots} />
          <button type="button" className={`icone-btn ${settingsOpen ? 'ativo' : ''}`} title={t.settings} aria-label={t.settings} aria-expanded={settingsOpen} onClick={() => setSettingsOpen(!settingsOpen)}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <rect x="4" y="5" width="16" height="14" rx="2.5" />
              <path d="M14 5v14" />
            </svg>
          </button>
        </header>

        <div className="chat-mensagens" ref={list}>
          {messages.length === 0 && !bot.busy && mail.length === 0 && (
            <div className="chat-vazio">
              <Avatar agent={face} size={72} state="idle" followPointer />
              <b>{speaker}</b>
              <span>{`${bot.name} · ${AGENT_LABEL[bot.agent]}`}</span>
              <p>{t.botEmpty}</p>
              <div className="chat-vazio-sugestoes">
                {t.botStarters.map((starter) => (
                  <button key={starter} className="chip" onClick={() => void send(starter)}>
                    {starter}
                  </button>
                ))}
              </div>
            </div>
          )}
          {messages.map((message, index) => (
            <BotLine
              key={`${message.ms}-${index}`}
              bot={bot}
              message={message}
              previous={messages[index - 1]}
              approvable={index === pending}
              onApprove={() => void bots.act('approve', { id: bot.id })}
              sent={sent}
              onSend={(project) => void bots.act('send', { id: bot.id, answer_ms: message.ms, project })}
            />
          ))}
          {mail.length > 0 && !bot.busy && (
            <div className="caixa" id={INBOX_ID}>
              {mail.map((item) => (
                <div key={item.id} className="correio">
                  <span className="correio-rotulo">{`${t.inboxTitle} · ${item.subject ?? ''}`}</span>
                  <Expandable text={item.body} />
                  <span className="acao">
                    <button className="cheio" onClick={() => void bots.act('handle', { id: bot.id, message: item.id }).then(inbox.clear)}>
                      {t.handleMail}
                    </button>
                    <span className="nota">{t.mailUntrusted}</span>
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>

        {unseen && (
          <button type="button" className="novas" onClick={() => jump()}>
            {t.newBelow}
          </button>
        )}
        {bot.busy && <ActivityBar face={face} speaker={speaker} doing={lastAsk?.kind === 'approve' ? t.botEditing : t.botReading} since={lastAsk?.ms ?? Date.now()} />}
        {!bot.busy && waiting > 0 && (
          <div className="esperando">
            <i className="ponto alerta" />
            <span>{[pending >= 0 ? t.waitingApproval : '', mail.length > 0 ? t.waitingMail.replace('{n}', String(mail.length)) : ''].filter(Boolean).join(' · ')}</span>
            <button type="button" onClick={see}>
              {t.see}
            </button>
          </div>
        )}
        <Composer
          draftKey={bot.id}
          placeholder={`${t.composeBot} ${speaker}`}
          maxLength={bots.state.max_text}
          busy={bot.busy}
          onSend={send}
          onStop={() => void bots.act('stop', { id: bot.id })}
        />
        {bots.error && !settingsOpen && <p className="nota failed chat-erro">{bots.error}</p>}
        {settingsOpen && <BotSettings key={bot.id} bot={bot} bots={bots} onClose={() => setSettingsOpen(false)} />}
        </div>
      </section>
    </main>
  )
}

/** The project's ai-memory inbox: asked when the chat opens, when a turn ends, and every so often. */
function useInbox(bot: Bot) {
  const [value, setValue] = useState<Inbox | null>(null)
  useEffect(() => {
    if (bot.busy) return undefined
    let alive = true
    const load = () => botInbox(bot.id).then((next) => alive && setValue(next)).catch(() => alive && setValue(null))
    void load()
    const timer = window.setInterval(() => void load(), INBOX_MS)
    return () => {
      alive = false
      window.clearInterval(timer)
    }
  }, [bot.id, bot.busy])
  return { value, clear: () => setValue(null) }
}

/** The answer whose proposal can still be approved: the latest one, if nothing but failures came after it. */
function pendingProposal(messages: BotMessage[]): number {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    const m = messages[i]
    if (m.kind === 'failed' || m.kind === 'sent' || m.kind === 'stopped') continue
    if (m.kind === 'approve') return -1
    return m.kind === 'answer' && m.data.reply?.proposal ? i : -1
  }
  return -1
}
