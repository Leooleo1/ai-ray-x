import { type FormEvent, useState } from 'react'
import type { Bot, BusyThread } from '../../api'
import { AGENT_LABEL } from '../../avatars'
import type { Bots } from '../../bots'
import { compact, t } from '../../i18n'
import type { ChatNav } from '../chat/nav'
import { TrailView } from '../chat/TrailView'
import { Who } from './Who'

const MAIN = 'main'
/** What a card shows of a turn's path before it is opened. */
const CARD_STEPS = 3
/** What a card shows of the text being written. */
const CARD_TEXT = 160

const clock = (ms: number) => {
  const seconds = Math.max(0, Math.round(ms / 1000))
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}

/** The conversation a running turn belongs to, as the person knows it. */
function workingIn(bot: Bot, thread: string): string {
  if (thread === 'review') return t.deskReviewing
  if (thread === MAIN) return t.mainConversation
  return bot.threads.find((x) => x.id === thread)?.title ?? ''
}

interface WorkCardProps {
  bot: Bot
  job: BusyThread
  bots: Bots
  nav: ChatNav
  now: number
  /** The board's task this turn is doing, when it is one. */
  task?: string
}

/** One AI at work: what it does now, for how long, its last steps, and the controls. */
export function WorkCard({ bot, job, bots, nav, now, task }: WorkCardProps) {
  const [open, setOpen] = useState(false)
  const [fixing, setFixing] = useState(false)
  const [text, setText] = useState('')
  const trail = job.trail
  const thread = job.thread && job.thread !== 'review' ? job.thread : MAIN
  const shown = trail && (open ? trail : { ...trail, items: trail.items.slice(-CARD_STEPS), dropped: 0 })
  const closeFix = () => {
    setFixing(false)
    setText('')
  }
  const correct = async (event: FormEvent) => {
    event.preventDefault()
    if (text.trim() && (await bots.act('correct', { id: bot.id, thread: job.thread, text })) !== null) closeFix()
  }
  return (
    <article className="mesa-cartao doing mesa-ao-vivo">
      <span className="mesa-tipo">
        <span>{`${AGENT_LABEL[bot.agent]} · ${workingIn(bot, job.thread)}`}</span>
        {job.started_ms > 0 && <time className="mesa-cifra">{clock(now - job.started_ms)}</time>}
      </span>
      {task && <p className="mesa-titulo">{task}</p>}
      <p className="mesa-passo">
        {job.step ? t.stepWords[job.step.kind] : t.botReading}
        {job.step?.target && <code>{job.step.target}</code>}
      </p>
      {shown && shown.items.length > 0 && <TrailView trail={shown} live />}
      {trail?.partial && <p className="mesa-formando">{trail.partial.length > CARD_TEXT ? `…${trail.partial.slice(-CARD_TEXT)}` : trail.partial}</p>}
      <Who bot={bot} working figure={trail?.tokens ? `${compact(trail.tokens)} tokens` : undefined} />
      {fixing && (
        <form className="mesa-corrigir" onSubmit={(event) => void correct(event)}>
          <input autoFocus value={text} maxLength={bots.state.max_text} placeholder={t.correctPlaceholder} onChange={(event) => setText(event.target.value)} onKeyDown={(event) => event.key === 'Escape' && closeFix()} />
          <button type="submit" className="mesa-cheio" disabled={!text.trim()}>
            {t.send}
          </button>
        </form>
      )}
      <footer className="mesa-acoes">
        <button type="button" className="mesa-cheio" onClick={() => nav.openMessage(bot.id, thread, 0)}>
          {t.deskOpen}
        </button>
        <button type="button" className="acao-texto" onClick={() => (fixing ? closeFix() : setFixing(true))}>
          {fixing ? t.cancel : t.correctSend}
        </button>
        <button type="button" className="acao-texto" onClick={() => void bots.act('stop', { id: bot.id, thread: job.thread })}>
          {t.stop}
        </button>
        {trail && trail.items.length > CARD_STEPS && (
          <button type="button" className="acao-texto mesa-caminho" onClick={() => setOpen(!open)}>
            {open ? t.deskLess : t.deskPath}
          </button>
        )}
      </footer>
    </article>
  )
}
