import { useState } from 'react'
import { AGENT_LABEL, faceForBot } from '../../avatars'
import type { Bots } from '../../bots'
import { t } from '../../i18n'
import { Avatar } from '../Avatar'
import { BotList } from './BotList'
import { Composer } from './Composer'
import type { ChatNav } from './nav'

interface AskManyProps {
  bots: Bots
  unread: Set<string>
  nav: ChatNav
}

/** One question to the bots the person picks; each answers in a new conversation, shown side by side. */
export function AskMany({ bots, unread, nav }: AskManyProps) {
  const free = bots.state.bots.filter((b) => !b.busy)
  const [picked, setPicked] = useState<Set<string>>(() => new Set(free.map((b) => b.id)))
  const toggle = (id: string) => setPicked((now) => (now.has(id) ? new Set([...now].filter((x) => x !== id)) : new Set([...now, id])))
  const send = async (text: string) => {
    if (picked.size === 0) return false
    const next = await bots.act('ask_many', { ids: [...picked], text })
    const batch = next?.batches[0]
    if (batch) nav.openBatch(batch.id)
    return next !== null
  }
  return (
    <main className="chat">
      <section className="cartao chat-centro bot-chat">
        <BotList bots={bots} current="grupo" unread={unread} nav={nav} />
        <div className="conversa">
          <header className="chat-topo bot-topo">
            <b className="grupo-titulo">{t.askManyTitle}</b>
          </header>
          <div className="chat-mensagens grupo-escolha">
            <p className="nota">{t.askManyHint}</p>
            <div className="grupo-bots" role="group" aria-label={t.askManyPick}>
              {bots.state.bots.map((bot) => (
                <button
                  key={bot.id}
                  type="button"
                  role="checkbox"
                  aria-checked={picked.has(bot.id)}
                  disabled={bot.busy}
                  className={picked.has(bot.id) ? 'ativo' : ''}
                  onClick={() => toggle(bot.id)}
                >
                  <Avatar agent={faceForBot(bot)} size={24} state="still" />
                  <span>
                    <b>{bot.name}</b>
                    <small>{bot.busy ? t.skippedBusy : AGENT_LABEL[bot.agent]}</small>
                  </span>
                  <i className="grupo-marca" aria-hidden="true" />
                </button>
              ))}
            </div>
          </div>
          <Composer
            draftKey="grupo"
            placeholder={`${t.askManySend} · ${picked.size} bots`}
            maxLength={bots.state.max_text}
            busy={false}
            onSend={send}
            onStop={() => undefined}
          />
          {bots.error && <p className="nota failed chat-erro">{bots.error}</p>}
        </div>
      </section>
    </main>
  )
}
