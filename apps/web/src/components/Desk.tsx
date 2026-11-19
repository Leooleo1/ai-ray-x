import { type FormEvent, useEffect, useState } from 'react'
import type { Bot } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import type { Bots } from '../bots'
import { compact, t } from '../i18n'
import { Avatar } from './Avatar'
import { when } from './chat/listParts'
import type { ChatNav } from './chat/nav'
import { TrailView } from './chat/TrailView'
import { waitingOf } from './chat/waiting'
import { TaskBoard } from './TaskBoard'
import './desk.css'

const MAIN = 'main'
/** What a card shows of a turn's path before it is opened. */
const CARD_STEPS = 3
/** What a card shows of the text being written. */
const CARD_TEXT = 160
const TICK_MS = 1000

function useNow(): number {
  const [now, setNow] = useState(Date.now())
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), TICK_MS)
    return () => window.clearInterval(timer)
  }, [])
  return now
}

const clock = (ms: number) => {
  const seconds = Math.max(0, Math.round(ms / 1000))
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}

/** The conversation a bot works in now, as the person knows it. */
function workingIn(bot: Bot): string {
  const thread = bot.busy_thread ?? MAIN
  if (thread === 'review') return t.deskReviewing
  if (thread === MAIN) return t.mainConversation
  return bot.threads.find((x) => x.id === thread)?.title ?? ''
}

interface DeskProps {
  bots: Bots
  nav: ChatNav
}

/** Every agent working now, ordered by project, then what waits for the person, then the whole team. */
export function Desk({ bots, nav }: DeskProps) {
  const now = useNow()
  const all = bots.state.bots
  const working = all.filter((b) => b.busy)
  // The team: by project, then by CLI.
  const team = [...all].sort((a, b) => a.name.localeCompare(b.name) || a.agent.localeCompare(b.agent))
  const waiting = all.flatMap(waitingOf).sort((a, b) => b.ms - a.ms)
  // One grid, ordered by project: cards of a project sit together without a row each.
  const ordered = [...working].sort((a, b) => a.name.localeCompare(b.name) || a.agent.localeCompare(b.agent))
  return (
    <main className="mesa">
      <TaskBoard bots={all} nav={nav} />
      <section className="mesa-secao">
        <h2>
          {t.deskWorking}
          {working.length > 0 && <small>{working.length}</small>}
        </h2>
        {working.length === 0 && <p className="mesa-vazio">{t.deskEmpty}</p>}
        {ordered.length > 0 && (
          <div className="mesa-grade">
            {ordered.map((bot) => (
              <WorkCard key={bot.id} bot={bot} bots={bots} nav={nav} now={now} />
            ))}
          </div>
        )}
      </section>

      {waiting.length > 0 && (
        <section className="mesa-secao">
          <h2>
            {t.deskWaiting}
            <small>{waiting.length}</small>
          </h2>
          <ul className="mesa-espera">
            {waiting.map((item) => (
              <li key={`${item.bot.id}:${item.thread}:${item.kind}:${item.ms}:${item.text}`}>
                <Avatar agent={faceForBot(item.bot)} size={22} state="still" />
                <span className="mesa-espera-onde">
                  <b>{item.bot.name}</b>
                  <small>{`${AGENT_LABEL[item.bot.agent]} · ${item.title}`}</small>
                </span>
                <span className={`mesa-espera-tipo ${item.kind}`}>{t.waitingKinds[item.kind]}</span>
                <code>{item.text}</code>
                <button type="button" className="vidro-btn" onClick={() => nav.openMessage(item.bot.id, item.thread, item.ms)}>
                  {t.deskOpen}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="mesa-secao">
        <h2>
          {t.deskTeam}
          <small>{all.length}</small>
        </h2>
        <ul className="mesa-equipe">
          {team.map((bot) => (
            <li key={bot.id}>
              <button type="button" onClick={() => nav.openBot(bot.id)}>
                <Avatar agent={faceForBot(bot)} size={28} state={bot.busy ? 'thinking' : 'still'} />
                <span className="mesa-equipe-quem">
                  <b>{bot.persona?.name ?? AGENT_LABEL[bot.agent]}</b>
                  <small className={bot.persona?.role ? '' : 'sem-cargo'}>{bot.persona?.role || t.deskNoRole}</small>
                </span>
                <span className="mesa-equipe-onde">
                  <b>{bot.name}</b>
                  <small>{[AGENT_LABEL[bot.agent], bot.model].filter(Boolean).join(' · ')}</small>
                </span>
                <span className="mesa-equipe-numeros">
                  <span className={`mesa-estado ${bot.busy ? 'ativo' : ''}`}>
                    <i className="ponto" />
                    {bot.busy ? t.deskWorkingNow : [t.deskIdleSince, when(lastActivity(bot))].filter(Boolean).join(' · ')}
                  </span>
                  <small>
                    {[weekTokens(bot, now) ? t.deskWeek.replace('{n}', compact(weekTokens(bot, now))) : '', bot.routines.length === 1 ? t.deskRoutine : bot.routines.length > 1 ? t.deskRoutines.replace('{n}', String(bot.routines.length)) : '']
                      .filter(Boolean)
                      .join(' · ')}
                  </small>
                </span>
              </button>
            </li>
          ))}
        </ul>
      </section>
    </main>
  )
}

const WEEK_MS = 7 * 86_400_000

/** What this agent read and wrote through raiox in the last seven days, across its conversations. */
function weekTokens(bot: Bot, now: number): number {
  return [...bot.messages, ...bot.threads.flatMap((x) => x.messages)]
    .filter((m) => m.author === 'bot' && m.ms >= now - WEEK_MS)
    .reduce((sum, m) => sum + (m.data.reply?.read_tokens ?? 0) + (m.data.reply?.output_tokens ?? 0), 0)
}

const lastActivity = (bot: Bot): number | undefined =>
  Math.max(0, ...[...bot.messages, ...bot.threads.flatMap((x) => x.messages)].map((m) => m.ms)) || undefined

interface WorkCardProps {
  bot: Bot
  bots: Bots
  nav: ChatNav
  now: number
}

/** One agent at work: where, what it does now, for how long, its last steps, and the controls. */
function WorkCard({ bot, bots, nav, now }: WorkCardProps) {
  const [open, setOpen] = useState(false)
  const [fixing, setFixing] = useState(false)
  const [text, setText] = useState('')
  const trail = bot.trail
  const thread = bot.busy_thread && bot.busy_thread !== 'review' ? bot.busy_thread : MAIN
  const shown = trail && (open ? trail : { ...trail, items: trail.items.slice(-CARD_STEPS), dropped: 0 })
  const correct = async (event: FormEvent) => {
    event.preventDefault()
    if (!text.trim()) return
    if ((await bots.act('correct', { id: bot.id, text })) !== null) {
      setText('')
      setFixing(false)
    }
  }
  return (
    <article className={`mesa-cartao ${open ? 'aberto' : ''}`}>
      <header>
        <Avatar agent={faceForBot(bot)} size={28} state="thinking" />
        <span className="mesa-quem">
          <b>{bot.name}</b>
          <small>{`${AGENT_LABEL[bot.agent]} · ${workingIn(bot)}`}</small>
        </span>
        <span className="mesa-tempo">{[bot.started_ms ? clock(now - bot.started_ms) : '', trail?.tokens ? `${compact(trail.tokens)} tokens` : ''].filter(Boolean).join(' · ')}</span>
      </header>
      <p className="mesa-passo">
        {bot.step ? t.stepWords[bot.step.kind] : t.botReading}
        {bot.step?.target && <code>{bot.step.target}</code>}
      </p>
      {shown && shown.items.length > 0 && <TrailView trail={shown} live />}
      {trail?.partial && <p className="mesa-formando">{trail.partial.length > CARD_TEXT ? `…${trail.partial.slice(-CARD_TEXT)}` : trail.partial}</p>}
      {fixing && (
        <form className="mesa-corrigir" onSubmit={(event) => void correct(event)}>
          <input autoFocus value={text} maxLength={bots.state.max_text} placeholder={t.correctPlaceholder} onChange={(event) => setText(event.target.value)} />
        </form>
      )}
      <footer>
        <button type="button" className="cheio" onClick={() => nav.openMessage(bot.id, thread, now)}>
          {t.deskOpen}
        </button>
        <button type="button" className="vidro-btn" onClick={() => setFixing(!fixing)}>
          {t.correctSend}
        </button>
        <button type="button" className="vidro-btn" onClick={() => void bots.act('stop', { id: bot.id })}>
          {t.stop}
        </button>
        {trail && trail.items.length > CARD_STEPS && (
          <button type="button" className="link" onClick={() => setOpen(!open)}>
            {open ? t.deskLess : t.deskPath}
          </button>
        )}
      </footer>
    </article>
  )
}
