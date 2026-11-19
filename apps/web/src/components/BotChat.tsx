import { useEffect, useRef, useState } from 'react'
import { type Bot, type BotMessage, type Inbox, type Place, type Report, botInbox } from '../api'
import { AGENT_LABEL, faceForBot, speakerOf } from '../avatars'
import type { Bots } from '../bots'
import { t } from '../i18n'
import { Avatar } from './Avatar'
import { BotSettings } from './BotSettings'
import { ActivityBar } from './chat/ActivityBar'
import { BotLine } from './chat/BotLine'
import { ProjectList } from './chat/ProjectList'
import { ModelPicker } from './chat/ModelPicker'
import { Composer } from './chat/Composer'
import { Expandable } from './chat/Expandable'
import { useFollowBottom } from './chat/useFollowBottom'
import { MemoriesLine } from './chat/MemoriesLine'
import { PinsLine } from './chat/PinsLine'
import { HouseAsk, HouseLine, isHouse } from './chat/HouseLine'
import type { ChatNav } from './chat/nav'
import { askToNotify } from '../unread'
import './chat/chat.css'

/** The project's ai-memory inbox is asked again this often while the chat is open. */
const INBOX_MS = 30_000
/** The conversation every bot has; the others are parallel ones. */
const MAIN = 'main'
const INBOX_ID = 'caixa-ai-memory'
/** Lets the conversation render before scrolling to a message in it. */
const FOCUS_DELAY_MS = 120
const PROPOSAL_ID = 'esperando-aprovacao'

interface BotChatProps {
  bot: Bot
  bots: Bots
  nav: ChatNav
  /** A conversation and message to open at (from search or a group question). */
  focus: { thread: string; ms: number } | null
  /** Opens Settings at one of its tabs. */
  onSettings: (tab: 'conexoes' | 'fios') => void
  unread: Set<string>
  /** For Dieta's cuts: the suggestions as computed now and the changes applied. */
  report: Report
  onChanged: () => void
}

/** A conversation with the person's own CLI in one project: read-only talk, edits after approval. */
export function BotChat({ bot, bots, nav, focus, onSettings, unread, report, onChanged }: BotChatProps) {
  const [settingsOpen, setSettingsOpen] = useState(false)
  // On a phone the project's conversations open as a sheet.
  const [listOpen, setListOpen] = useState(false)
  const [thread, setThread] = useState(focus?.thread ?? MAIN)
  useEffect(() => {
    if (!focus) return undefined
    setThread(focus.thread)
    const timer = window.setTimeout(() => document.getElementById(`msg-${focus.ms}`)?.scrollIntoView({ block: 'center' }), FOCUS_DELAY_MS)
    return () => window.clearTimeout(timer)
  }, [focus])
  const [modelSignal, setModelSignal] = useState(0)
  const [memorySignal, setMemorySignal] = useState(0)
  const inbox = useInbox(bot)
  const list = useRef<HTMLDivElement>(null)
  const side = bot.threads.find((x) => x.id === thread)
  // A conversation closed elsewhere falls back to the main one.
  const current = side ? thread : MAIN
  const messages = side ? side.messages : bot.messages
  // One turn at a time per bot: busy here, or busy in another conversation.
  const busyHere = bot.busy && (bot.busy_thread ?? MAIN) === current
  const busyElsewhere = bot.busy && !busyHere
  // ai-memory mail always lands in the main conversation.
  const mail = current === MAIN ? (inbox.value?.messages ?? []) : []
  const { unseen, jump } = useFollowBottom(list, messages.length + mail.length + (busyHere ? 1 : 0), `${bot.id}:${current}`)
  const openPlace = (place: Place) => {
    if (place === 'model') setModelSignal((n) => n + 1)
    else if (place === 'settings') setSettingsOpen(true)
    else if (place === 'memories') setMemorySignal((n) => n + 1)
    else onSettings(place === 'wires' ? 'fios' : 'conexoes')
  }
  const newThread = async () => {
    const next = await bots.act('thread_new', { id: bot.id })
    const created = next?.bots.find((b) => b.id === bot.id)?.threads.at(-1)
    if (created) setThread(created.id)
  }
  const closeThread = async (id: string) => {
    if ((await bots.act('thread_delete', { id: bot.id, thread: id })) && id === current) setThread(MAIN)
  }
  const face = faceForBot(bot)
  const speaker = speakerOf(bot)
  const pending = pendingProposal(messages)
  const sent = new Set(messages.filter((m) => m.kind === 'sent').map((m) => `${m.data.answer_ms}|${m.data.project}`))
  const lastAsk = [...messages].reverse().find((m) => m.author === 'you')
  // Each draft's latest fate: applied, undone or discarded; open until one of those.
  const draftStates = new Map(
    messages
      .filter((m) => m.kind === 'draft_applied' || m.kind === 'draft_undone' || m.kind === 'draft_discarded')
      .map((m) => [m.data.draft ?? '', m.kind === 'draft_applied' ? ('applied' as const) : m.kind === 'draft_undone' ? ('undone' as const) : ('discarded' as const)]),
  )
  const waiting = (pending >= 0 ? 1 : 0) + mail.length

  const send = async (text: string) => {
    askToNotify()
    return (await bots.act('ask', { id: bot.id, thread: current, text })) !== null
  }
  const see = () => document.getElementById(pending >= 0 ? PROPOSAL_ID : INBOX_ID)?.scrollIntoView({ behavior: 'smooth', block: 'center' })

  return (
    <main className="chat">
      <section className="cartao chat-centro bot-chat">
        <ProjectList
          bots={bots}
          project={bot.name}
          current={`${bot.id}:${current}`}
          unread={unread}
          nav={nav}
          open={listOpen}
          onThread={(id, thread) => {
            setListOpen(false)
            if (id === bot.id) setThread(thread)
            else nav.openMessage(id, thread, 0)
          }}
          onNew={() => {
            setListOpen(false)
            void newThread()
          }}
          onClose={(id, thread) => void (id === bot.id ? closeThread(thread) : bots.act('thread_delete', { id, thread }))}
        />
        <div className="conversa">
        <header className="chat-topo bot-topo">
          <button type="button" className="icone-btn so-celular" aria-label={t.conversationsOf.replace('{p}', bot.name)} aria-expanded={listOpen} onClick={() => setListOpen(!listOpen)}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M5 7h14M5 12h14M5 17h9" />
            </svg>
          </button>
          <span className="conversa-titulo">
            <Avatar agent={face} size={24} state={busyHere ? 'thinking' : 'still'} />
            <span>
              <b>{side ? side.title : t.mainConversation}</b>
              <small>{[bot.name, AGENT_LABEL[bot.agent], bot.persona?.name].filter(Boolean).join(' · ')}</small>
            </span>
          </span>
          <ModelPicker bot={bot} bots={bots} openSignal={modelSignal} />
          <button type="button" className={`icone-btn ${settingsOpen ? 'ativo' : ''}`} title={t.settings} aria-label={t.settings} aria-expanded={settingsOpen} onClick={() => setSettingsOpen(!settingsOpen)}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <rect x="4" y="5" width="16" height="14" rx="2.5" />
              <path d="M14 5v14" />
            </svg>
          </button>
        </header>

        <div className="linhas-topo">
          <MemoriesLine bot={bot} openSignal={memorySignal} />
          <PinsLine bot={bot} onGo={(pin) => nav.openMessage(bot.id, pin.thread, pin.ms)} />
        </div>
        <div className="chat-mensagens" ref={list}>
          {messages.length === 0 && !busyHere && mail.length === 0 && (
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
          {messages.map((message, index) =>
            isHouse(message.author) ? (
              <HouseLine key={`${message.ms}-${index}`} bot={bot} message={message} messages={messages} report={report} onChanged={onChanged} />
            ) : (
            <BotLine
              key={`${message.ms}-${index}`}
              bot={bot}
              message={message}
              previous={messages[index - 1]}
              approvable={index === pending}
              onApprove={() => void bots.act('approve', { id: bot.id, thread: current })}
              sent={sent}
              onSend={(project) => void bots.act('send', { id: bot.id, answer_ms: message.ms, project })}
              onOpenExchange={nav.openExchange}
              onOpenPlace={openPlace}
              draftState={message.kind === 'draft' ? draftStates.get(message.data.draft ?? '') ?? 'open' : undefined}
              onDraft={(action) => void bots.act('draft', { id: bot.id, thread: current, draft: message.data.draft, action })}
              pinned={bot.pins.some((p) => p.thread === current && p.ms === message.ms)}
              onPin={() => void bots.act('pin', { id: bot.id, thread: current, ms: message.ms })}
            />
            ),
          )}
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
        {busyElsewhere && <p className="linha-sistema">{t.busyElsewhere}</p>}
        {busyHere && <ActivityBar face={face} speaker={speaker} doing={lastAsk?.kind === 'approve' ? t.botEditing : t.botReading} step={bot.step} since={lastAsk?.ms ?? Date.now()} />}
        {!bot.busy && waiting > 0 && (
          <div className="esperando">
            <i className="ponto alerta" />
            <span>{[pending >= 0 ? t.waitingApproval : '', mail.length > 0 ? t.waitingMail.replace('{n}', String(mail.length)) : ''].filter(Boolean).join(' · ')}</span>
            <button type="button" onClick={see}>
              {t.see}
            </button>
          </div>
        )}
        {!busyHere && <HouseAsk onAsk={(agent) => void bots.act('house', { id: bot.id, thread: current, agent })} />}
        <Composer
          draftKey={`${bot.id}:${current}`}
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
    if (m.kind === 'failed' || m.kind === 'sent' || m.kind === 'stopped' || m.kind === 'house') continue
    if (m.kind === 'approve') return -1
    return m.kind === 'answer' && m.data.reply?.proposal ? i : -1
  }
  return -1
}
