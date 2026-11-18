import { useRef } from 'react'
import { faceForBot } from '../../avatars'
import type { Bots } from '../../bots'
import { type Exchange as ExchangeData, type ExchangeItem } from '../../exchanges'
import { t } from '../../i18n'
import { Avatar } from '../Avatar'
import { Markdown } from '../Markdown'
import { BotList } from './BotList'
import { clock } from './BotLine'
import { Expandable } from './Expandable'
import { useFollowBottom } from './useFollowBottom'

interface ExchangeProps {
  exchange: ExchangeData
  bots: Bots
  onOpen: (id: string) => void
  onOpenExchange: (a: string, b: string) => void
  onAdd: () => void
  unread: Set<string>
}

/** One project's bot and another's, side by side: left the one that started, right the other; the person's steps in the middle. */
export function ExchangeView({ exchange, bots, onOpen, onOpenExchange, onAdd, unread }: ExchangeProps) {
  const list = useRef<HTMLDivElement>(null)
  useFollowBottom(list, exchange.items.length, `${exchange.a}|${exchange.b}`)
  const botOf = (project: string) => bots.state.bots.find((b) => b.name === project)
  const ends = [exchange.a, exchange.b].map((project) => ({ project, bot: botOf(project) }))
  return (
    <main className="chat">
      <section className="cartao chat-centro bot-chat">
        <BotList bots={bots} current={`${exchange.a}|${exchange.b}`} unread={unread} onOpen={onOpen} onOpenExchange={onOpenExchange} onAdd={onAdd} />
        <div className="conversa">
          <header className="chat-topo bot-topo troca-topo">
            {ends.map(({ project, bot }, i) => (
              <span key={project} className="troca-ponta">
                {i === 1 && <span className="troca-seta" aria-hidden="true">⇄</span>}
                <button type="button" className="bot-aba" disabled={!bot} onClick={() => bot && onOpen(bot.id)}>
                  {bot && <Avatar agent={faceForBot(bot)} size={22} state="still" />}
                  <b>{project}</b>
                </button>
              </span>
            ))}
            <small>{`${t.throughMemory} · ${exchange.items.length} ${t.steps2}${exchange.waiting > 0 ? ` · ${exchange.waiting} ${t.waitingToHandle}` : ''}`}</small>
          </header>
          <div className="chat-mensagens troca" ref={list}>
            {exchange.items.map((item, index) => (
              <Item key={`${item.kind}-${item.ms}-${index}`} item={item} left={exchange.a} />
            ))}
          </div>
        </div>
      </section>
    </main>
  )
}

function Item({ item, left }: { item: ExchangeItem; left: string }) {
  if (item.kind === 'sent') {
    return <p className="linha-sistema">{`${t.youSentFromTo.replace('{a}', item.from).replace('{b}', item.to)} · ${clock(item.ms)}`}</p>
  }
  const side = (item.kind === 'received' ? item.to : item.from) === left ? 'esquerda' : 'direita'
  const face = faceForBot(item.bot)
  const label = { suggested: t.suggestedFor, received: t.handedTo, answered: t.answered }[item.kind]
  return (
    <div className={`troca-linha ${side}`}>
      <Avatar agent={face} size={28} state="still" />
      <div className="corpo">
        <span className="quem">
          <b style={{ color: face.color }}>{item.bot.name}</b>
          <span className="hora">{`${label} · ${clock(item.ms)}`}</span>
        </span>
        <div className={`bolha ${item.kind === 'received' ? 'recebida' : ''}`}>
          {item.kind === 'answered' ? <Markdown text={item.text} color={face.color} /> : <Expandable text={item.text} />}
        </div>
        {item.kind === 'answered' && item.proposal && <span className="bot-meta">{`${t.proposal}: ${item.proposal.replace(/`/g, '')}`}</span>}
      </div>
    </div>
  )
}
