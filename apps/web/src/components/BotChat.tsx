import { useEffect, useRef, useState } from 'react'
import { type Bot, type Inbox, type Place, type Report, botInbox, wikiFromAnswer } from '../api'
import { AGENT_LABEL, faceForBot, speakerOf, personaName } from '../avatars'
import type { Bots } from '../bots'
import { t } from '../i18n'
import { Avatar } from './Avatar'
import { BotSettings } from './BotSettings'
import { ActivityBar } from './chat/ActivityBar'
import { CarryMenu } from './chat/Carry'
import { PreviewPanel } from './chat/PreviewPanel'
import { latestAnswer, pendingProposal } from './chat/waiting'
import { TrailView } from './chat/TrailView'
import { ReviewLine } from './chat/ReviewLine'
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
  const [previewOpen, setPreviewOpen] = useState(false)
  const toggleSettings = () => {
    setSettingsOpen((was) => {
      if (!was) setPreviewOpen(false)
      return !was
    })
  }
  const togglePreview = () => {
    setPreviewOpen((was) => {
      if (!was) setSettingsOpen(false)
      return !was
    })
  }
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
  // The live path and the text being written grow while the bot works: the view follows them.
  const growing = busyHere ? 1 + (bot.trail?.items.length ?? 0) + Math.floor((bot.trail?.partial.length ?? 0) / 60) : 0
  const { unseen, jump } = useFollowBottom(list, messages.length + mail.length + growing, `${bot.id}:${current}`)
  // Run on an AI's See the app card: the panel opens and starts the commands at once.
  const [runSignal, setRunSignal] = useState(0)
  const openPlace = (place: Place, run = false) => {
    if (place === 'model') setModelSignal((n) => n + 1)
    else if (place === 'settings') {
      setPreviewOpen(false)
      setSettingsOpen(true)
    } else if (place === 'app') {
      setSettingsOpen(false)
      setPreviewOpen(true)
      if (run) setRunSignal((n) => n + 1)
    } else if (place === 'access') void giveAccess()
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
  const pendingRun = latestAnswer(messages)
  const partners = bots.state.bots.filter((b) => b.dir === bot.dir && b.id !== bot.id)
  const reviewState = (ms: number): 'none' | 'pending' | 'done' => {
    const about = messages.filter((m) => m.data.answer_ms === ms || (m.kind === 'failed' && m.data.review === ms))
    if (about.some((m) => m.kind === 'review')) return 'done'
    const asked = about.findLast((m) => m.kind === 'review_asked')
    return asked && !about.some((m) => m.kind === 'failed' && m.ms > asked.ms) ? 'pending' : 'none'
  }
  const reviewerOf = (ms: number) => {
    const asked = messages.findLast((m) => m.kind === 'review_asked' && m.data.answer_ms === ms)
    const partner = bots.state.bots.find((b) => b.id === asked?.data.reviewer)
    return partner ? AGENT_LABEL[partner.agent] : ''
  }
  const kept = new Set(messages.filter((m) => m.kind === 'remembered').map((m) => m.data.answer_ms))
  const sent = new Set(messages.filter((m) => m.kind === 'sent').map((m) => `${m.data.answer_ms}|${m.data.project}`))
  const lastAsk = [...messages].reverse().find((m) => m.author === 'you')
  // Each draft's latest fate: applied, undone or discarded; open until one of those.
  const draftStates = new Map(
    messages
      .filter((m) => m.kind === 'draft_applied' || m.kind === 'draft_undone' || m.kind === 'draft_discarded')
      .map((m) => [m.data.draft ?? '', m.kind === 'draft_applied' ? ('applied' as const) : m.kind === 'draft_undone' ? ('undone' as const) : ('discarded' as const)]),
  )
  const waiting = (pending >= 0 ? 1 : 0) + mail.length

  const send = async (text: string, images?: string[]) => {
    askToNotify()
    const payload: Record<string, unknown> = { id: bot.id, thread: current, text }
    if (images && images.length > 0) payload.images = images
    return (await bots.act('ask', payload)) !== null
  }
  /** Full access from the conversation: on, then the AI is told it may go on. */
  const giveAccess = async () => {
    if ((await bots.act('grants', { id: bot.id, full_access: true })) !== null) await send(t.accessContinue)
  }
  const toggleAccess = () => void bots.act('grants', { id: bot.id, full_access: !bot.grants?.full_access })
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
              <small>{[bot.name, AGENT_LABEL[bot.agent], personaName(bot)].filter(Boolean).join(' · ')}</small>
            </span>
          </span>
          <button type="button" className={`icone-btn ${previewOpen ? 'ativo' : ''}`} title={t.previewOpen} aria-label={t.previewOpen} aria-expanded={previewOpen} onClick={togglePreview}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <rect x="3.5" y="5" width="17" height="12" rx="2" />
              <path d="M9 20h6M12 17v3" />
            </svg>
          </button>
          {messages.length > 0 && <CarryMenu bot={bot} thread={current} bots={bots} nav={nav} />}
          <ModelPicker bot={bot} bots={bots} openSignal={modelSignal} />
          <button type="button" data-abre-ajustes className={`icone-btn ${settingsOpen ? 'ativo' : ''}`} title={t.settings} aria-label={t.settings} aria-expanded={settingsOpen} onClick={toggleSettings}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <rect x="4" y="5" width="16" height="14" rx="2.5" />
              <path d="M14 5v14" />
            </svg>
          </button>
        </header>

        <div className="linhas-topo">
          <MemoriesLine bot={bot} openSignal={memorySignal} version={kept.size} />
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
            message.kind === 'review' || message.kind === 'review_asked' ? (
              <ReviewLine key={`${message.ms}-${index}`} message={message} reviewer={bots.state.bots.find((b) => b.id === message.data.reviewer)} />
            ) : isHouse(message.author) ? (
              <HouseLine key={`${message.ms}-${index}`} bot={bot} bots={bots} message={message} messages={messages} report={report} onChanged={onChanged} />
            ) : (
            <BotLine
              key={`${message.ms}-${index}`}
              bot={bot}
              message={message}
              previous={messages[index - 1]}
              approvable={index === pending}
              runnable={index === pendingRun}
              partners={partners}
              reviewState={reviewState(message.ms)}
              reviewerName={reviewerOf(message.ms)}
              onReview={(reviewer) => void bots.act('review', { id: bot.id, thread: current, answer_ms: message.ms, reviewer })}
              remembered={kept.has(message.ms)}
              onRemember={() => void bots.act('remember', { id: bot.id, thread: current, answer_ms: message.ms })}
              onWiki={() => wikiFromAnswer(bot.id, current, message.ms).then(() => true, () => false)}
              onRun={(which, always) => void bots.act('run', { id: bot.id, thread: current, index: which, always })}
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
          {busyHere && bot.trail && (bot.trail.items.length > 0 || bot.trail.partial) && (
            <div className="linha de-agente ao-vivo" aria-live="polite">
              <div className="corpo">
                {bot.trail.items.length > 0 && <TrailView trail={bot.trail} live />}
                {bot.trail.partial && (
                  <p className="formando">
                    {bot.trail.partial}
                    <i aria-hidden="true" />
                  </p>
                )}
              </div>
            </div>
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
          allowImages
          onSend={send}
          onStop={() => void bots.act('stop', { id: bot.id })}
          access={bot.grants ? { on: bot.grants.full_access, onToggle: toggleAccess } : undefined}
          onCorrect={busyHere ? async (text) => (await bots.act('correct', { id: bot.id, text })) !== null : undefined}
        />
        {bots.error && !settingsOpen && <p className="nota failed chat-erro">{bots.error}</p>}
        {settingsOpen && (
          <BotSettings
            key={bot.id}
            bot={bot}
            bots={bots}
            onClose={() => setSettingsOpen(false)}
            onOpenThread={(threadId) => {
              setSettingsOpen(false)
              setThread(threadId)
            }}
          />
        )}
        {previewOpen && <PreviewPanel key={bot.id} bot={bot} bots={bots} runSignal={runSignal} onClose={() => setPreviewOpen(false)} />}
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
/** The latest answer, when nothing came after it: the commands it asks for can still run. */
