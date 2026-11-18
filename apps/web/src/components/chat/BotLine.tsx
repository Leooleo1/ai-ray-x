import { useState } from 'react'
import type { Bot, BotMessage } from '../../api'
import { faceForBot, speakerOf } from '../../avatars'
import { compact, locale, t } from '../../i18n'
import { Avatar } from '../Avatar'
import { Markdown } from '../Markdown'
import { Expandable } from './Expandable'

/** Answers from the bot this close together read as one block. */
const GROUP_MS = 5 * 60_000
/** After a pause this long, the time stands above the next message again. */
const PAUSE_MS = 60 * 60_000
const COPIED_MS = 1400

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

const answerLike = (m: BotMessage | undefined) => m?.author === 'bot' && (m.kind === 'answer' || m.kind === 'failed' || m.kind === 'stopped')

interface BotLineProps {
  bot: Bot
  message: BotMessage
  previous: BotMessage | undefined
  isLatest: boolean
  approvable: boolean
  onApprove: () => void
  /** "answer ms|project" of the suggestions already sent. */
  sent: Set<string>
  onSend: (project: string) => void
}

export function BotLine({ bot, message, previous, isLatest, approvable, onApprove, sent, onSend }: BotLineProps) {
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
  if (message.author === 'you' && message.kind === 'sent') {
    return <p className="linha-sistema">{t.youSent.replace('{p}', message.data.project ?? '')}</p>
  }
  if (message.author === 'you') {
    const fromMail = message.data.via === 'ai-memory'
    return (
      <>
        {head}
        <div className="linha minha">
          <span className={`balao ${fromMail ? 'via-memoria' : ''}`}>
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
  const grouped = !mark && answerLike(previous) && message.ms - (previous?.ms ?? 0) < GROUP_MS
  const reply = message.data.reply
  const meta = reply && [
    `${t.readWord} ${compact(reply.read_tokens)} tokens`,
    reply.steps ? `${reply.steps} ${t.steps}` : '',
    `${Math.max(1, Math.round(reply.duration_ms / 1000))} s`,
  ]
  return (
    <>
      {head}
      <div className={`linha de-agente ${grouped ? 'agrupada' : ''}`}>
        {grouped ? <span className="recuo" /> : <Avatar agent={face} size={32} state={isLatest ? 'idle' : 'still'} />}
        <div className="corpo">
          {!grouped && (
            <span className="quem">
              <b style={{ color: face.color }}>{speakerOf(bot)}</b>
              <span className="hora">{clock(message.ms)}</span>
              {message.data.mode === 'edit' && <span className="editou">{t.editedBadge}</span>}
            </span>
          )}
          {message.kind === 'failed' && <p className="nota failed quebra">{message.data.error}</p>}
          {reply && <Markdown text={reply.text} color={face.color} />}
          {meta && (
            <span className="bot-meta">
              {meta.filter(Boolean).join(' · ')}
              <CopyButton text={reply.text} />
            </span>
          )}
          {reply?.proposal && (
            <div className="proposta" id={approvable ? 'esperando-aprovacao' : undefined}>
              <span className="proposta-rotulo">{t.proposal}</span>
              <Markdown text={reply.proposal} />
              {approvable && (
                <span className="acao">
                  <button className="cheio" disabled={bot.busy} onClick={onApprove}>
                    {t.approveEdit}
                  </button>
                  <span className="nota">{bot.agent === 'codex' ? t.codexEditScope : t.claudeEditScope}</span>
                </span>
              )}
            </div>
          )}
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
