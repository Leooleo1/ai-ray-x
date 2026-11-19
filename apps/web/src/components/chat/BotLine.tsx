import { useState } from 'react'
import type { Bot, BotMessage, Place } from '../../api'
import { faceForBot } from '../../avatars'
import { compact, locale, t } from '../../i18n'
import { Markdown } from '../Markdown'
import { DraftCard, type DraftState } from './DraftCard'
import { RanBlock, RunRequestCard } from './RunCards'
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
  onOpenPlace: (place: Place) => void
  pinned: boolean
  onPin: () => void
  /** For a draft: whether it was applied, undone or discarded, and the way to do it. */
  draftState?: DraftState
  onDraft?: (action: 'apply' | 'discard' | 'undo') => void
  /** The latest answer: the commands it asks for can still run. */
  runnable: boolean
  onRun: (index: number, always: boolean) => void
}

export function BotLine({ bot, message, previous, approvable, onApprove, sent, onSend, onOpenExchange, onOpenPlace, pinned, onPin, draftState, onDraft, runnable, onRun }: BotLineProps) {
  const face = faceForBot(bot)
  const mark = separator(previous, message)
  const head = mark && (
    <div className="dia">
      <span>{mark}</span>
    </div>
  )
  if (message.kind === 'reset') {
    return (
      <div className="dia">
        <span>{t.conversationReset}</span>
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
  if (message.author === 'you') {
    const fromMail = message.data.via === 'ai-memory'
    return (
      <>
        {head}
        <div className="linha minha" id={`msg-${message.ms}`}>
          <span className={`balao ${fromMail ? 'via-memoria' : 'na-cor'}`} style={fromMail ? undefined : { background: face.color }}>
            {fromMail ? (
              <>
                <small>{`${t.fromMemory} · ${message.data.text ?? ''}`}</small>
                <Expandable text={message.data.body ?? ''} />
              </>
            ) : (
              message.data.text
            )}
          </span>
          <span className="hora">{clock(message.ms)}</span>
        </div>
      </>
    )
  }
  if (message.kind === 'stopped') {
    return <p className="linha-sistema">{t.stoppedNote}</p>
  }
  const reply = message.data.reply
  const meta = reply && [
    `${t.readWord} ${compact(reply.read_tokens)} tokens`,
    reply.steps ? `${reply.steps} ${reply.steps === 1 ? t.stepOne : t.steps}` : '',
    `${Math.max(1, Math.round(reply.duration_ms / 1000))} s`,
  ]
  return (
    <>
      {head}
      <div className="linha de-agente bolha-linha" id={`msg-${message.ms}`}>
        <div className="corpo">
          {message.kind === 'failed' && <p className="nota failed quebra">{message.data.error}</p>}
          {reply && (
            <div className="bolha">
              <Markdown text={reply.text} color={face.color} />
            </div>
          )}
          {meta && (
            <span className="bot-meta">
              {message.data.mode === 'edit' && <span className="editou">{t.editedBadge}</span>}
              {[clock(message.ms), ...meta].filter(Boolean).join(' · ')}
              <CopyButton text={reply.text} />
              <button type="button" className={`copiar ${pinned ? 'fixada' : ''}`} aria-pressed={pinned} onClick={onPin}>
                {pinned ? t.pinned : t.pin}
              </button>
            </span>
          )}
          {message.kind === 'draft' && onDraft && <DraftCard message={message} state={draftState ?? 'open'} busy={bot.busy} onAct={onDraft} />}
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
              <button type="button" className="vidro-btn" onClick={() => onOpenPlace(place)}>
                {t.open}
              </button>
            </div>
          ))}
          {reply?.proposal && (
            <div className="proposta" id={approvable ? 'esperando-aprovacao' : undefined}>
              <span className="proposta-rotulo">{t.proposal}</span>
              <Markdown text={reply.proposal} />
              {approvable && (
                <span className="acao">
                  <button className="cheio" disabled={bot.busy} onClick={onApprove}>
                    {t.approveEdit}
                  </button>
                  <span className="nota">{t.draftScope}</span>
                </span>
              )}
            </div>
          )}
          {reply?.run?.map((request, index) => (
            <RunRequestCard key={request.command} request={request} live={runnable} busy={bot.busy} onRun={(always) => onRun(index, always)} />
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

/** Copies an answer; shows on hover and keyboard focus. */
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
