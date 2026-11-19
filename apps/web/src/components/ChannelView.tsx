import { useRef, useState } from 'react'
import type { Bot, Channel, ChannelItem } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import type { Bots } from '../bots'
import { t } from '../i18n'
import { Avatar } from './Avatar'
import { Composer } from './chat/Composer'
import { Markdown } from './Markdown'
import type { ChatNav } from './chat/nav'
import { TrailView } from './chat/TrailView'
import { useFollowBottom } from './chat/useFollowBottom'
import './channel.css'

/** What the live row shows of a member's path. */
const LIVE_STEPS = 3

interface ChannelViewProps {
  channel: Channel
  bots: Bots
  nav: ChatNav
  onDeleted: () => void
}

/** A channel: the person's messages and the members' answers together, who is working now, and the requests to pass on. */
export function ChannelView({ channel, bots, nav, onDeleted }: ChannelViewProps) {
  const list = useRef<HTMLDivElement>(null)
  const [deleting, setDeleting] = useState(false)
  const members = channel.members.map((m) => ({ ...m, bot: bots.state.bots.find((b) => b.id === m.bot) })).filter((m): m is typeof m & { bot: Bot } => m.bot !== undefined)
  const handleOf = (id: string) => channel.members.find((m) => m.bot === id)?.handle ?? id
  const lead = members[0]
  const working = members.filter((m) => m.bot.busy && m.bot.busy_thread === channel.threads[m.bot.id])
  const growing = working.reduce((sum, m) => sum + 1 + (m.bot.trail?.items.length ?? 0), 0)
  const { unseen, jump } = useFollowBottom(list, channel.messages.length + growing, channel.id)
  const passed = (answer: number, index: number) => channel.messages.some((item) => item.message.kind === 'pass' && item.message.data.answer_ms === answer && item.message.data.index === index)
  const post = async (text: string) => (await bots.act('channel_post', { channel: channel.id, text })) !== null
  const remove = async () => {
    if (!deleting) return setDeleting(true)
    if (await bots.act('channel_delete', { channel: channel.id })) onDeleted()
  }
  return (
    <main className="chat">
      <section className="cartao chat-centro bot-chat sem-lista canal">
        <div className="conversa">
          <header className="chat-topo bot-topo canal-topo">
            <span className="canal-nome">{`# ${channel.name}`}</span>
            <span className="canal-membros">
              {members.map((m, index) => (
                <button key={m.bot.id} type="button" title={`${m.bot.name} · ${AGENT_LABEL[m.bot.agent]}`} onClick={() => nav.openBot(m.bot.id)}>
                  <Avatar agent={faceForBot(m.bot)} size={20} state={m.bot.busy ? 'thinking' : 'still'} />
                  <span>{`@${m.handle}`}</span>
                  {index === 0 && <small>{t.channelLead}</small>}
                </button>
              ))}
            </span>
            <button type="button" className={`canal-apagar ${deleting ? 'confirmar' : ''}`} onClick={() => void remove()} onBlur={() => setDeleting(false)}>
              {deleting ? t.channelDeleteSure : t.channelDelete}
            </button>
          </header>
          <div className="chat-mensagens" ref={list}>
            {channel.messages.length === 0 && <p className="canal-vazio">{t.channelEmpty.replace('{lead}', lead ? `@${lead.handle}` : '')}</p>}
            {channel.messages.map((item, index) => (
              <Line key={`${item.message.ms}-${index}`} item={item} bots={bots} nav={nav} handleOf={handleOf} passed={passed} channel={channel.id} />
            ))}
            {working.map((m) => (
              <div key={m.bot.id} className="linha de-agente canal-ao-vivo" aria-live="polite">
                <Avatar agent={faceForBot(m.bot)} size={28} state="thinking" />
                <div className="corpo">
                  <span className="canal-quem">
                    <b style={{ color: faceForBot(m.bot).color }}>{`@${m.handle}`}</b>
                    <small>{m.bot.step ? `${t.stepWords[m.bot.step.kind]} ${m.bot.step.target}` : t.botReading}</small>
                  </span>
                  {m.bot.trail && m.bot.trail.items.length > 0 && <TrailView trail={{ ...m.bot.trail, items: m.bot.trail.items.slice(-LIVE_STEPS), dropped: 0 }} live />}
                  {m.bot.trail?.partial && <p className="formando">{m.bot.trail.partial}</p>}
                </div>
              </div>
            ))}
          </div>
          {unseen && (
            <button type="button" className="novas" onClick={() => jump()}>
              {t.newBelow}
            </button>
          )}
          <Composer
            draftKey={`canal:${channel.id}`}
            placeholder={t.channelCompose}
            maxLength={bots.state.max_text}
            busy={false}
            onSend={post}
            onStop={() => undefined}
          />
          {bots.error && <p className="nota failed chat-erro">{bots.error}</p>}
        </div>
      </section>
    </main>
  )
}

interface LineProps {
  item: ChannelItem
  bots: Bots
  nav: ChatNav
  handleOf: (id: string) => string
  passed: (answer: number, index: number) => boolean
  channel: string
}

/** One message of the channel: the person's, a pass, or a member's answer with its requests. */
function Line({ item, bots, nav, handleOf, passed, channel }: LineProps) {
  const { message } = item
  if (item.bot === null && message.kind === 'pass') {
    const to = typeof message.data.to === 'string' ? message.data.to : ''
    return <p className="linha-sistema">{t.channelPassed.replace('{from}', `@${handleOf(message.data.from ?? '')}`).replace('{to}', `@${handleOf(to)}`)}</p>
  }
  if (item.bot === null) {
    const to = (Array.isArray(message.data.to) ? message.data.to : []).map((id) => `@${handleOf(id)}`).join(', ')
    return (
      <div className="linha minha">
        <span className="balao na-cor canal-balao">
          {message.data.text}
          {to && <small>{t.channelTo.replace('{to}', to)}</small>}
        </span>
      </div>
    )
  }
  const bot = bots.state.bots.find((b) => b.id === item.bot)
  if (!bot) return null
  const face = faceForBot(bot)
  const handle = `@${handleOf(bot.id)}`
  if (message.kind !== 'answer') {
    if (message.kind === 'failed') return <p className="linha-sistema">{`${handle}: ${message.data.error ?? ''}`}</p>
    if (message.kind === 'stopped') return <p className="linha-sistema">{`${handle}: ${t.stoppedNote}`}</p>
    return null
  }
  const reply = message.data.reply
  if (!reply) return null
  return (
    <div className="linha de-agente bolha-linha">
      <Avatar agent={face} size={28} state="still" />
      <div className="corpo">
        <span className="canal-quem">
          <b style={{ color: face.color }}>{handle}</b>
          <small>{[bot.name, AGENT_LABEL[bot.agent], bot.persona?.role].filter(Boolean).join(' · ')}</small>
        </span>
        <div className="bolha">
          <Markdown text={reply.text} color={face.color} />
        </div>
        {reply.proposal && (
          <div className="proposta">
            <span className="proposta-rotulo">{t.proposal}</span>
            <Markdown text={reply.proposal} />
            <span className="acao">
              <button type="button" className="vidro-btn" onClick={() => nav.openMessage(bot.id, item.thread ?? 'main', message.ms)}>
                {t.channelOpenToApprove}
              </button>
            </span>
          </div>
        )}
        {(reply.ask ?? []).map((ask, index) => (
          <div key={index} className="correio canal-pedido">
            <span className="correio-rotulo">{t.channelAsks.replace('{from}', handle).replace('{to}', `@${ask.project}`)}</span>
            <Markdown text={ask.text} />
            <span className="acao">
              {passed(message.ms, index) ? (
                <span className="estado">
                  <i className="ponto ok" />
                  {t.channelPassedShort}
                </span>
              ) : (
                <button type="button" className="cheio" onClick={() => void bots.act('channel_pass', { channel, id: bot.id, answer_ms: message.ms, index })}>
                  {t.channelPass.replace('{to}', `@${ask.project}`)}
                </button>
              )}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

/** A new channel: a name and its members, the first one answering when nobody is mentioned. */
export function ChannelNew({ bots, onCreated }: { bots: Bots; onCreated: (id: string) => void }) {
  const [name, setName] = useState('')
  const [chosen, setChosen] = useState<string[]>([])
  const toggle = (id: string) => setChosen((before) => (before.includes(id) ? before.filter((x) => x !== id) : [...before, id]))
  const create = async () => {
    const next = await bots.act('channel_new', { name, members: chosen })
    const made = next?.channels.filter((c) => c.name === name.trim().replace(/^#\s*/, '')).sort((a, b) => b.created_ms - a.created_ms)[0]
    if (made) onCreated(made.id)
  }
  return (
    <main className="chat">
      <section className="cartao canal-novo">
        <h2>{t.channelNewTitle}</h2>
        <p className="nota">{t.channelNewHint}</p>
        <label className="ajustes-campo">
          <span>{t.channelName}</span>
          <input value={name} maxLength={40} placeholder={t.channelNameHint} onChange={(event) => setName(event.target.value)} />
        </label>
        <span className="ajustes-grupo">{t.channelMembers}</span>
        <ul className="canal-escolha">
          {bots.state.bots.map((bot) => {
            const order = chosen.indexOf(bot.id)
            return (
              <li key={bot.id}>
                <button type="button" aria-pressed={order >= 0} className={order >= 0 ? 'ativo' : ''} onClick={() => toggle(bot.id)}>
                  <Avatar agent={faceForBot(bot)} size={24} state="still" />
                  <span>
                    <b>{bot.name}</b>
                    <small>{[AGENT_LABEL[bot.agent], bot.persona?.role].filter(Boolean).join(' · ')}</small>
                  </span>
                  {order === 0 && <small className="canal-lider">{t.channelLead}</small>}
                </button>
              </li>
            )
          })}
        </ul>
        <span className="acao">
          <button type="button" className="cheio" disabled={!name.trim() || chosen.length === 0} onClick={() => void create()}>
            {t.channelCreate}
          </button>
          <span className="nota">{t.channelLeadHint}</span>
        </span>
        {bots.error && <p className="nota failed">{bots.error}</p>}
      </section>
    </main>
  )
}
