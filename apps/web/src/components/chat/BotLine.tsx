import { useState } from 'react'
import type { Bot, BotMessage, Place, Trail } from '../../api'
import { AGENT_LABEL, faceForBot } from '../../avatars'
import { compact, locale, t } from '../../i18n'
import { Markdown } from '../Markdown'
import { DraftCard, type DraftState } from './DraftCard'
import { TurnChanges } from './TurnChanges'
import { CarriedBlock } from './Carry'
import type { CancellableKind } from './waiting'
import { RanBlock, RunRequestCard } from './RunCards'
import { ShotBlock, UploadedImages } from './ShotBlock'
import { TrailView } from './TrailView'
import { Expandable } from './Expandable'

/** After a pause this long, the time stands above the next message again. */
const PAUSE_MS = 60 * 60_000
const COPIED_MS = 1400

const PLACE_ICONS: Record<Place, string> = {
  model: 'M12 3l2.2 5.6L20 9.2l-4.4 3.9 1.3 5.9L12 16l-4.9 3 1.3-5.9L4 9.2l5.8-.6z',
  settings: 'M4 7h9M17 7h3M4 17h3M11 17h9M15 5v4M9 15v4',
  memories: 'M6 4h12v16l-6-4-6 4z',
  connections: 'M9 7H7a5 5 0 0 0 0 10h2M15 7h2a5 5 0 0 1 0 10h-2M8 12h8',
  wires: 'M6 6a2 2 0 1 0 0 .01M18 18a2 2 0 1 0 0 .01M18 6a2 2 0 1 0 0 .01M7.5 7.5l9 9M8 6h8',
  app: 'M4 5h16v11H4zM9 20h6M12 16v4',
  access: 'M5 11h14v10H5zM8 11V7a4 4 0 0 1 7.5-2',
}

export const clock = (ms: number) => new Date(ms).toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })

function dayLabel(ms: number): string {
  const date = new Date(ms)
  if (date.toDateString() === new Date().toDateString()) return t.today
  return date.toLocaleDateString(locale, { weekday: 'long', day: '2-digit', month: 'long' })
}

/** The separator above a message: a new day, or the time after a long pause. */
export function separator(previous: BotMessage | undefined, message: BotMessage): string | null {
  if (!previous || dayLabel(previous.ms) !== dayLabel(message.ms)) return dayLabel(message.ms)
  return message.ms - previous.ms > PAUSE_MS ? clock(message.ms) : null
}

interface BotLineProps {
  bot: Bot
  message: BotMessage
  previous: BotMessage | undefined
  approvable: boolean
  onApprove: () => void
  /** "answer ms|project" of the suggestions already sent. */
  sent: Set<string>
  onSend: (project: string) => void
  onOpenExchange: (a: string, b: string) => void
  /** Opens a place; for the app, `run` also starts its dev commands. */
  onOpenPlace: (place: Place, run?: boolean) => void
  pinned: boolean
  onPin: () => void
  /** For a draft: whether it was applied, undone or discarded, and the way to do it. */
  draftState?: DraftState
  onDraft?: (action: 'apply' | 'discard' | 'undo') => void
  /** The conversation, for a full-access turn's diff; whether that turn was undone, and how to undo it. */
  thread?: string
  turnUndone?: boolean
  onTurnUndo?: () => void
  /** The latest answer can still be waiting for a decision. */
  waiting: boolean
  cancelled: (kind: CancellableKind, index?: number) => boolean
  onCancel: (kind: CancellableKind, index?: number) => void
  onRun: (index: number, always: boolean) => void
  /** Already kept in ai-memory as a decision of the project. */
  remembered: boolean
  onRemember: () => void
  /** A new conversation from this answer. */
  onBranch?: () => void
  /** Keeps the answer as a page of the team's wiki. */
  onWiki?: () => Promise<boolean>
  /** The other AIs of the project, which can review a proposal. */
  partners: Bot[]
  reviewState: 'none' | 'pending' | 'done'
  reviewerName: string
  onReview: (reviewer: string) => void
  /** Latest interrupted turn when the bot is idle: offers Continue / Try again. */
  resumable?: boolean
  onContinue?: () => void
  onRetry?: () => void
}

export function BotLine({ bot, message, previous, approvable, onApprove, sent, onSend, onOpenExchange, onOpenPlace, pinned, onPin, draftState, onDraft, thread, turnUndone = false, onTurnUndo, onBranch, waiting, cancelled, onCancel, onRun, remembered, onRemember, onWiki, partners, reviewState, reviewerName, onReview, resumable = false, onContinue, onRetry }: BotLineProps) {
  const face = faceForBot(bot)
  const mark = separator(previous, message)
  const head = mark && (
    <div className="dia">
      <span>{mark}</span>
    </div>
  )
  if (message.kind === 'reset') {
    return (
      <div className="dia aviso">
        <span>{t.conversationReset}</span>
      </div>
    )
  }
  if (message.kind === 'switched') {
    const target = typeof message.data.to === 'string' ? message.data.to : ''
    const to = (AGENT_LABEL as Record<string, string>)[target] ?? target
    const model = message.data.model ? ` · ${message.data.model}` : ''
    return (
      <div className="dia aviso">
        <span>{t.switchedTo.replace('{agent}', `${to}${model}`)}</span>
      </div>
    )
  }
  if (message.author === 'you' && message.kind === 'approve') {
    return (
      <>
        {head}
        <p className="linha-sistema">{`${t.approved}: ${(message.data.proposal ?? '').replace(/`/g, '')}`}</p>
      </>
    )
  }
  if (message.kind === 'shot') {
    return (
      <>
        {head}
        <ShotBlock bot={bot} message={message} />
      </>
    )
  }
  if (message.kind === 'carried') {
    return (
      <>
        {head}
        <CarriedBlock message={message} />
      </>
    )
  }
  if (message.author === 'you' && message.kind === 'remembered') {
    return (
      <p className="linha-sistema" title={message.data.path}>
        {t.rememberedLine}
      </p>
    )
  }
  if (message.author === 'you' && message.kind === 'ran') {
    return (
      <>
        {head}
        <RanBlock message={message} />
      </>
    )
  }
  if (message.author === 'you' && message.kind.startsWith('draft_')) {
    const paths = message.data.paths ?? []
    const what = message.kind === 'draft_applied' ? t.youApplied : message.kind === 'draft_undone' ? t.youUndid : t.youDiscarded
    return <p className="linha-sistema">{what.replace('{n}', paths.join(', '))}</p>
  }
  if (message.author === 'you' && message.kind === 'sent') {
    const project = message.data.project ?? ''
    return (
      <p className="linha-sistema">
        {t.youSent.replace('{p}', project)}{' '}
        <button type="button" className="ver-mais" onClick={() => onOpenExchange(bot.name, project)}>
          {t.seeExchange}
        </button>
      </p>
    )
  }
  if (message.kind === 'waiting_cancelled') {
    return <p className="linha-sistema">{t.youCancelled}</p>
  }
  if (message.author === 'you' && message.data.continued) {
    return <p className="linha-sistema">{t.continuedLine}</p>
  }
  if (message.author === 'you') {
    const fromMail = message.data.via === 'ai-memory'
    const images = message.data.images ?? []
    const text = message.data.text ?? ''
    const bubble = (fromMail || text) && (
      <span className={`balao ${fromMail ? 'via-memoria' : 'na-cor'}`} style={fromMail ? undefined : { background: face.color }}>
        {fromMail ? (
          <>
            <small>{`${t.fromMemory} · ${text}`}</small>
            <Expandable text={message.data.body ?? ''} />
          </>
        ) : (
          text
        )}
      </span>
    )
    return (
      <>
        {head}
        <div className={`linha minha ${images.length > 0 ? 'com-imagens' : ''} ${message.data.aside ? 'pergunta-lateral' : ''}`} id={`msg-${message.ms}`}>
          {images.length > 0 ? (
            <div className="minha-conteudo">
              <UploadedImages bot={bot} names={images} />
              {bubble}
            </div>
          ) : (
            bubble
          )}
          <span className="hora">{clock(message.ms)}</span>
        </div>
      </>
    )
  }
  if (message.kind === 'stopped') {
    return <p className="linha-sistema">{t.stoppedNote}</p>
  }
  if (message.kind === 'interrupted') {
    const trail = message.data.trail
    const hasTrail = Boolean(trail && (trail.items.length > 0 || trail.partial))
    const cursorModeChanged = bot.agent === 'cursor' && typeof message.data.full_access === 'boolean' && message.data.full_access !== Boolean(bot.grants?.full_access)
    const canContinue = Boolean(message.data.session) && !cursorModeChanged
    const canRetry = Boolean(message.data.retry_text)
    return (
      <div className="interrompido" id={`msg-${message.ms}`}>
        {hasTrail && trail && (
          <div className="interrompido-trabalho">
            {trail.items.length > 0 && <TrailView trail={trail} live={false} />}
            {trail.partial && <p className="interrompido-parcial">{trail.partial}</p>}
          </div>
        )}
        <div className="interrompido-barra">
          <span className="linha-sistema">{t.interruptedNote}</span>
          {resumable && (canContinue || canRetry) && (
            <span className="interrompido-acoes">
              {canContinue && onContinue && (
                <button type="button" className="projeto-acao cheio" onClick={onContinue}>
                  {t.continueTurn}
                </button>
              )}
              {canRetry && onRetry && (
                <button type="button" className={canContinue ? 'acao-texto' : 'projeto-acao cheio'} onClick={onRetry}>
                  {t.retryTurn}
                </button>
              )}
            </span>
          )}
        </div>
      </div>
    )
  }
  if (message.kind === 'access_changed') {
    return <p className="linha-sistema">{message.data.full ? t.accessChangedFull : t.accessChangedAsk}</p>
  }
  if (message.kind === 'forked') {
    return <p className="linha-sistema">{t.forkedLine}</p>
  }
  if (message.kind === 'turn_undone') {
    return <p className="linha-sistema">{t.turnUndoneLine}</p>
  }
  const reply = message.data.reply
  const meta = reply && [
    `${t.readWord} ${compact(reply.read_tokens)} tokens`,
    reply.steps && !reply.trail ? `${reply.steps} ${reply.steps === 1 ? t.stepOne : t.steps}` : '',
    `${Math.max(1, Math.round(reply.duration_ms / 1000))} s`,
  ]
  return (
    <>
      {head}
      <div className={`linha de-agente bolha-linha ${message.data.aside ? 'pergunta-lateral' : ''}`} id={`msg-${message.ms}`}>
        <div className="corpo">
          {message.kind === 'failed' && <p className="nota failed quebra">{message.data.error}</p>}
          {reply?.text && (
            <div className="bolha">
              <Markdown text={reply.text} color={face.color} />
            </div>
          )}
          {meta && (
            <span className="bot-meta">
              {message.data.mode === 'edit' && <span className="editou">{t.editedBadge}</span>}
              {message.data.aside && <span className="editou">{t.asideBadge}</span>}
              {message.data.audit && message.data.auditor_agent && (
                <span className="auditor-selo">{t.auditorBadge.replace('{agent}', AGENT_LABEL[message.data.auditor_agent] ?? message.data.auditor_agent)}</span>
              )}
              {/* Each part stays whole ("12 s", "62 mil tokens"); the line breaks only between parts. */}
              <span className="bot-meta-texto">{[clock(message.ms), ...meta].filter(Boolean).map((part) => part.replace(/ /g, '\u00a0')).join(' · ')}</span>
              <span className="bot-meta-acoes">
                <CopyButton text={reply.text} />
                <button type="button" className={`copiar ${pinned ? 'fixada' : ''}`} aria-pressed={pinned} onClick={onPin}>
                  {pinned ? t.pinned : t.pin}
                </button>
                {message.kind === 'answer' && (
                  <button type="button" className={`copiar ${remembered ? 'fixada' : ''}`} disabled={remembered} onClick={onRemember}>
                    {remembered ? t.remembered : t.remember}
                  </button>
                )}
                {message.kind === 'answer' && onWiki && <WikiKeep onWiki={onWiki} />}
                {message.kind === 'answer' && onBranch && !message.data.aside && (
                  <button type="button" className="copiar" title={t.branchHint} onClick={onBranch}>
                    {t.branchHere}
                  </button>
                )}
              </span>
            </span>
          )}
          {message.kind === 'draft' && onDraft && <DraftCard message={message} state={draftState ?? 'open'} busy={bot.busy} onAct={onDraft} />}
          {message.data.changes && thread && onTurnUndo && (
            <TurnChanges bot={bot.id} thread={thread} ms={message.ms} changes={message.data.changes} undone={turnUndone} busy={bot.busy} onUndo={onTurnUndo} />
          )}
          {reply?.trail && <AnswerPath trail={reply.trail} />}
          {reply?.open?.map((place) => (
            <div key={place} className="cartao-acao">
              <span className="cartao-acao-icone" aria-hidden="true">
                <svg viewBox="0 0 24 24">
                  <path d={PLACE_ICONS[place]} />
                </svg>
              </span>
              <span className="cartao-acao-texto">
                <b>{t.places[place]}</b>
                <small>{`${bot.name} › ${t.placeWhere[place]}`}</small>
              </span>
              {place === 'app' && (
                <button type="button" className="cheio" onClick={() => onOpenPlace(place, true)}>
                  {t.previewRun}
                </button>
              )}
              {/* Full access in one click: given, and the AI goes on with what it was asked. */}
              {place === 'access' && !bot.grants?.full_access && (
                <button type="button" className="cheio" onClick={() => onOpenPlace(place, true)}>
                  {t.accessGive}
                </button>
              )}
              {place === 'access' && bot.grants?.full_access && <small className="cartao-acao-feito">{t.accessGiven}</small>}
              {place !== 'access' && (
                <button type="button" className="vidro-btn" onClick={() => onOpenPlace(place)}>
                  {t.open}
                </button>
              )}
            </div>
          ))}
          {reply?.proposal && !cancelled('proposal') && (
            <div className="proposta" id={approvable ? 'esperando-aprovacao' : undefined}>
              <span className="proposta-rotulo">{t.proposal}</span>
              <Markdown text={reply.proposal} />
              {approvable && (
                <span className="acao">
                  <button className="cheio" disabled={bot.busy} onClick={onApprove}>
                    {t.approveEdit}
                  </button>
                  <button type="button" className="link" disabled={bot.busy} onClick={() => onCancel('proposal')}>
                    {t.cancel}
                  </button>
                  <span className="nota">{t.draftScope}</span>
                </span>
              )}
              {approvable && reviewState === 'none' && partners.length > 0 && (
                <span className="acao">
                  {partners.map((partner) => (
                    <button key={partner.id} type="button" className="link" disabled={partner.busy} onClick={() => onReview(partner.id)}>
                      {t.reviewAsk.replace('{a}', AGENT_LABEL[partner.agent])}
                    </button>
                  ))}
                </span>
              )}
              {reviewState === 'pending' && <span className="nota revisando">{t.reviewing.replace('{a}', reviewerName)}</span>}
            </div>
          )}
          {reply?.question && (waiting && !cancelled('question') ? (
            <div className="proposta">
              <span className="proposta-rotulo">{t.waitingKinds.question}</span>
              <Markdown text={reply.question} />
              <span className="acao">
                <span className="nota">{t.waitingReplyHint}</span>
                <button type="button" className="link" disabled={bot.busy} onClick={() => onCancel('question')}>{t.cancel}</button>
              </span>
            </div>
          ) : <Markdown text={reply.question} />)}
          {reply?.run?.map((request, index) => (
            <RunRequestCard key={request.command} request={request} live={waiting && !cancelled('command', index)} busy={bot.busy}
              onRun={(always) => onRun(index, always)} onCancel={() => onCancel('command', index)} />
          ))}
          {reply?.to?.map((note) => {
            const done = sent.has(`${message.ms}|${note.project}`)
            return (
              <div key={note.project} className="correio">
                <span className="correio-rotulo">{t.mailTo.replace('{p}', note.project)}</span>
                <Expandable text={note.text} />
                <span className="acao">
                  {done ? (
                    <span className="estado">
                      <i className="ponto ok" />
                      {t.sentMail}
                    </span>
                  ) : (
                    <button className="vidro-btn" disabled={bot.busy} onClick={() => onSend(note.project)}>
                      {t.sendMail}
                    </button>
                  )}
                </span>
              </div>
            )
          })}
        </div>
      </div>
    </>
  )
}

/** The path an answer took, folded under it: opens on a tap. */
function AnswerPath({ trail }: { trail: Trail }) {
  const steps = trail.items.filter((item) => item.kind !== 'say').length + trail.dropped
  return (
    <details className="caminho">
      <summary>{steps === 1 ? t.trailOpenOne : t.trailOpen.replace('{n}', String(steps))}</summary>
      <TrailView trail={trail} live={false} />
    </details>
  )
}

/** Copies an answer; shows on hover and keyboard focus. */
/** Keeps an answer as a wiki page, once. */
function WikiKeep({ onWiki }: { onWiki: () => Promise<boolean> }) {
  const [kept, setKept] = useState(false)
  return (
    <button type="button" className={`copiar ${kept ? 'fixada' : ''}`} disabled={kept} onClick={() => void onWiki().then(setKept)}>
      {kept ? t.wikiFromAnswerDone : t.wikiFromAnswer}
    </button>
  )
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  const copy = () =>
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true)
      window.setTimeout(() => setCopied(false), COPIED_MS)
    })
  return (
    <button type="button" className="copiar" onClick={copy}>
      {copied ? t.copiedShort : t.copy}
    </button>
  )
}
