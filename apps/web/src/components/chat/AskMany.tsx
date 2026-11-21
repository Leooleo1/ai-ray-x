import { useState } from 'react'
import { AGENT_LABEL, faceForBot } from '../../avatars'
import type { Bots } from '../../bots'
import { t } from '../../i18n'
import { Avatar } from '../Avatar'
import { count, when } from './listParts'
import { Composer } from './Composer'
import type { ChatNav } from './nav'

interface AskManyProps {
  bots: Bots
  nav: ChatNav
}

/** One question to the bots the person picks; each answers in a new conversation, shown side by side. */
export function AskMany({ bots, nav }: AskManyProps) {
  const free = bots.state.bots
  const [picked, setPicked] = useState<Set<string>>(() => new Set(free.map((b) => b.id)))
  const toggle = (id: string) => setPicked((now) => (now.has(id) ? new Set([...now].filter((x) => x !== id)) : new Set([...now, id])))
  const send = async (text: string) => {
    if (picked.size === 0) return false
    const next = await bots.act('ask_many', { ids: [...picked], text })
    const batch = next?.batches[0]
    if (batch) nav.openBatch(batch.id)
    return next !== null
  }
  const allFreePicked = free.length > 0 && free.every((b) => picked.has(b.id))
  const toggleAll = () => setPicked(allFreePicked ? new Set() : new Set(free.map((b) => b.id)))
  return (
    <main className="chat">
      <section className="cartao chat-centro bot-chat sem-lista">
        <div className="conversa">
          <header className="chat-topo bot-topo">
            <b className="grupo-titulo">{t.askManyTitle}</b>
          </header>
          <div className="chat-mensagens grupo-escolha">
            <div className="grupo-cabeca">
              <p className="nota">{t.askManyHint}</p>
              {free.length > 1 && (
                <button type="button" className="lado-link" onClick={toggleAll}>
                  {allFreePicked ? t.askManyNone : t.askManyAll}
                </button>
              )}
            </div>
            <div className="grupo-bots" role="group" aria-label={t.askManyPick}>
              {bots.state.bots.map((bot) => (
                <button
                  key={bot.id}
                  type="button"
                  role="checkbox"
                  aria-checked={picked.has(bot.id)}
                  className={picked.has(bot.id) ? 'ativo' : ''}
                  onClick={() => toggle(bot.id)}
                >
                  <Avatar agent={faceForBot(bot)} size={24} state="still" />
                  <span>
                    <b>{bot.name}</b>
                    <small>{AGENT_LABEL[bot.agent]}</small>
                  </span>
                  <i className="grupo-marca" aria-hidden="true" />
                </button>
              ))}
            </div>
            {bots.state.batches.length > 0 && (
              <section className="grupo-anteriores">
                <span className="conversas-grupo">{t.previousGroups}</span>
                <ul>
                  {bots.state.batches.map((batch) => (
                    <li key={batch.id}>
                      <button type="button" onClick={() => nav.openBatch(batch.id)}>
                        <b>{batch.text}</b>
                        <small>{`${count(batch.items.length, t.aisCount)} · ${when(batch.created_ms)}`}</small>
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </div>
          <Composer
            draftKey="grupo"
            placeholder={`${t.askManySend} · ${count(picked.size, t.aisCount)}`}
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
