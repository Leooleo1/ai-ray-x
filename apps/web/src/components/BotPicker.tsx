import { type CSSProperties, useCallback, useState } from 'react'
import { type Bot, busyIn, isEditing, projectBots } from '../api'
import { AGENT_SHORT, faceForBot } from '../avatars'
import type { Bots } from '../bots'
import { t } from '../i18n'
import { AddProjectModal } from './AddProjectModal'
import { AgentLogo } from './AgentLogo'
import { Avatar } from './Avatar'
import { type Waiting, waitingOf } from './chat/waiting'

function relativeTime(ms: number): string {
  const mins = Math.max(0, Math.floor((Date.now() - ms) / 60_000))
  if (mins < 1) return t.planNow
  if (mins < 60) return t.planMinutes.replace('{n}', String(mins))
  const hours = Math.floor(mins / 60)
  if (hours < 24) return t.planHours.replace('{n}', String(hours))
  const days = Math.floor(hours / 24)
  return days === 1 ? t.planDayOne : t.planDays.replace('{n}', String(days))
}

function previewOf(bot: Bot): string | null {
  if (bot.last_line) return bot.last_line
  for (let i = bot.messages.length - 1; i >= 0; i -= 1) {
    const m = bot.messages[i]
    const raw = m.kind === 'answer' || m.kind === 'draft' ? m.data.reply?.text : m.kind === 'ask' ? m.data.text : m.kind === 'failed' ? m.data.error : undefined
    const line = raw?.split('\n').map((l) => l.trim()).find(Boolean)
    if (line) return line
  }
  return null
}

function lastTimeOf(bot: Bot): number | null {
  if (typeof bot.last_ms === 'number') return bot.last_ms
  return bot.messages.length > 0 ? bot.messages[bot.messages.length - 1].ms : null
}

interface ActiveCard {
  bot: Bot
  waiting: boolean
  busy: boolean
  lastMs: number | null
  preview: string | null
}

/** Active bots ordered: waiting for you first, then working, then most recent. */
function activeCards(list: Bot[]): ActiveCard[] {
  return list
    .map((bot) => ({
      bot,
      waiting: waitingOf(bot).length > 0,
      busy: bot.busy,
      lastMs: lastTimeOf(bot),
      preview: previewOf(bot),
    }))
    .sort((a, b) => {
      const rank = (card: ActiveCard) => (card.waiting ? 0 : card.busy ? 1 : 2)
      return rank(a) - rank(b) || (b.lastMs ?? 0) - (a.lastMs ?? 0) || a.bot.name.localeCompare(b.bot.name)
    })
}

interface BotPickerProps {
  bots: Bots
  onOpen: (id: string) => void
  onAskMany: () => void
  /** Opens a channel, or the new-channel form with `novo`. */
  onChannel: (id: string) => void
}

/** Active projects as live cards, with a final card that opens the add-project modal. */
/** Faces a state shows at most beside its count. */
const MAX_PULSE_FACES = 4

export function BotPicker({ bots, onOpen, onAskMany, onChannel }: BotPickerProps) {
  const [modalOpen, setModalOpen] = useState(false)
  const closeModal = useCallback(() => setModalOpen(false), [])
  const cards = activeCards(projectBots(bots.state.bots))
  const empty = cards.length === 0

  return (
    <main className="chat">
      <section className="cartao projetos-pagina">
        {!empty && (
          <>
            {/* On a phone the menu has no room for it, so Ask several lives here too. */}
            <button type="button" className="projetos-varios so-celular" onClick={onAskMany}>
              {t.askMany}
            </button>
            <TeamPulse cards={cards} onOpen={onOpen} />
          </>
        )}
        <div className={`projetos-grade ${empty ? 'vazia' : ''}`}>
          {cards.map(({ bot, waiting, busy, lastMs, preview }) => (
            <BotRoom key={bot.id} bot={bot} waiting={waiting} busy={busy} lastMs={lastMs} preview={preview} onOpen={() => onOpen(bot.id)} bots={bots} />
          ))}
          <button
            type="button"
            className={`projeto-cartao projeto-cartao-adicionar ${empty ? 'destaque' : ''}`}
            onClick={() => setModalOpen(true)}
          >
            <span className="projeto-cartao-mais">
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M12 5v14M5 12h14" />
              </svg>
              <b>{t.addProject}</b>
            </span>
            {empty && <span className="projeto-cartao-convite">{t.addFirstProjectHint}</span>}
          </button>
        </div>
        {!empty && (
          <>
            {/* On a phone the menu has no room for the channels either, so they live here too. */}
            <h2 className="projetos-secao so-celular">{t.channels}</h2>
            <ul className="projetos-lista so-celular">
              {(bots.state.channels ?? []).map((channel) => (
                <li key={channel.id} className="projeto-linha">
                  <span className="projeto-cerquilha" aria-hidden="true">
                    #
                  </span>
                  <span className="projeto-nome">
                    <b>{channel.name}</b>
                    <small>{channel.members.map((m) => `@${m.handle}`).join(' ')}</small>
                  </span>
                  <button type="button" className="acao-texto" onClick={() => onChannel(channel.id)}>
                    {t.openProject}
                  </button>
                </li>
              ))}
              <li className="projeto-linha">
                <span className="projeto-cerquilha" aria-hidden="true">
                  #
                </span>
                <span className="projeto-nome">
                  <b>{t.channelNewNav}</b>
                  <small>{t.channelNewHint}</small>
                </span>
                <button type="button" className="projeto-acao" onClick={() => onChannel('novo')}>
                  {t.channelCreate}
                </button>
              </li>
            </ul>
          </>
        )}
        {bots.error && !modalOpen && <p className="nota failed">{bots.error}</p>}
      </section>
      {modalOpen && <AddProjectModal bots={bots} onOpen={onOpen} onClose={closeModal} />}
    </main>
  )
}

interface BotRoomProps {
  bot: Bot
  waiting: boolean
  busy: boolean
  lastMs: number | null
  preview: string | null
  onOpen: () => void
  bots: Bots
}

/** A bot's card as its room: its character large on a floor of its own light, which brightens while it works or waits,
 *  and, while it works, the step it is on now; otherwise the last thing said. */
function BotRoom({ bot, waiting, busy, lastMs, preview, onOpen, bots }: BotRoomProps) {
  const face = faceForBot(bot)
  const step = busy ? bot.step : null
  const state = waiting ? t.cardWaiting : busy ? t.cardWorking : lastMs ? relativeTime(lastMs) : t.cardEmpty
  const first = waiting ? waitingOf(bot)[0] : undefined
  return (
    <div
      role="link"
      tabIndex={0}
      className={`projeto-cartao projeto-quarto ${waiting ? 'projeto-cartao-espera' : ''} ${busy ? 'ativo' : ''}`}
      style={{ '--cor-bot': face.color } as CSSProperties}
      onClick={onOpen}
      onKeyDown={(event) => event.key === 'Enter' && event.target === event.currentTarget && onOpen()}
    >
      <span className="quarto-personagem" aria-hidden="true">
        <Avatar agent={face} size={64} state={busy ? 'thinking' : waiting ? 'happy' : 'idle'} followPointer />
      </span>
      <span className="quarto-texto">
        <span className="projeto-cartao-topo">
          <b className="projeto-cartao-nome">{bot.name}</b>
          <span className="projeto-cartao-ia">
            <AgentLogo agent={bot.agent} size={14} />
            <span>{AGENT_SHORT[bot.agent]}</span>
          </span>
        </span>
        <span className={`projeto-cartao-estado ${waiting ? 'espera' : busy ? 'trabalha' : ''}`}>
          {waiting && <i className="ponto alerta" aria-hidden="true" />}
          {!waiting && busy && <i className="ponto trabalhando" aria-hidden="true" />}
          <span>{state}</span>
        </span>
        {step ? (
          <span className="projeto-cartao-frase quarto-passo">
            {t.stepWords[step.kind]}
            {step.target && <code>{step.target}</code>}
          </span>
        ) : (
          preview && <span className="projeto-cartao-frase">{preview}</span>
        )}
        {first && <QuickReply bot={bot} waiting={first} bots={bots} />}
      </span>
    </div>
  )
}

/** What a waiting bot asks, answered from its card: a proposal can be approved, a question or proposal answered in a line;
 *  a command or a draft needs the conversation, which the card opens. */
function QuickReply({ bot, waiting, bots }: { bot: Bot; waiting: Waiting; bots: Bots }) {
  const [text, setText] = useState('')
  const [sending, setSending] = useState(false)
  const answerable = waiting.kind === 'proposal' || waiting.kind === 'question'
  const send = async (action: 'ask' | 'approve') => {
    setSending(true)
    const payload = action === 'ask' ? { id: bot.id, thread: waiting.thread, text } : { id: bot.id, thread: waiting.thread, answer_ms: waiting.ms }
    if ((await bots.act(action, payload)) !== null) setText('')
    setSending(false)
  }
  return (
    // The card opens the conversation; clicks here stay here.
    <span className="quarto-resposta" onClick={(event) => event.stopPropagation()} onKeyDown={(event) => event.stopPropagation()}>
      <span className="quarto-pergunta">{waiting.text}</span>
      {answerable && (
        <form
          className="quarto-campo"
          onSubmit={(event) => {
            event.preventDefault()
            if (text.trim() && !sending) void send('ask')
          }}
        >
          <input value={text} placeholder={t.quickReplyPlaceholder} aria-label={t.quickReplyPlaceholder} disabled={sending} onChange={(event) => setText(event.target.value)} />
          {waiting.kind === 'proposal' && !text.trim() ? (
            <button type="button" className="cheio" disabled={sending || Boolean(busyIn(bot, waiting.thread)) || isEditing(bot)} onClick={() => void send('approve')}>
              {t.requestApprove}
            </button>
          ) : (
            <button type="submit" className="cheio" disabled={sending || Boolean(busyIn(bot, waiting.thread)) || !text.trim()}>
              {t.send}
            </button>
          )}
        </form>
      )}
    </span>
  )
}

/** How many of the team are in each state. */
type Pulse = 'waiting' | 'working' | 'idle'

/** The page's head: the team at a glance, a count per state with the faces in it; the bots waiting
 *  for the person are a click away (the first one opens). */
function TeamPulse({ cards, onOpen }: { cards: ActiveCard[]; onOpen: (id: string) => void }) {
  const groups: Record<Pulse, ActiveCard[]> = {
    waiting: cards.filter((c) => c.waiting),
    working: cards.filter((c) => !c.waiting && c.busy),
    idle: cards.filter((c) => !c.waiting && !c.busy),
  }
  const words: Record<Pulse, string> = { waiting: t.pulseWaiting, working: t.pulseWorking, idle: t.pulseIdle }
  const shown = (['waiting', 'working', 'idle'] as Pulse[]).filter((key) => groups[key].length > 0)
  return (
    <h2 className="equipe-pulso">
      {shown.map((key) => {
        const content = (
          <>
            <b className="pulso-numero">{groups[key].length}</b>
            <span className="pulso-rostos" aria-hidden="true">
              {groups[key].slice(0, MAX_PULSE_FACES).map((c) => (
                <Avatar key={c.bot.id} agent={faceForBot(c.bot)} size={20} state="still" />
              ))}
            </span>
            <span>{key === 'waiting' && groups.waiting.length === 1 ? t.pulseWaitingOne : words[key]}</span>
          </>
        )
        return key === 'waiting' ? (
          <button key={key} type="button" className={`pulso ${key}`} onClick={() => onOpen(groups.waiting[0].bot.id)}>
            {content}
          </button>
        ) : (
          <span key={key} className={`pulso ${key}`}>
            {content}
          </span>
        )
      })}
    </h2>
  )
}
