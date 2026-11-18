import { useEffect, useRef, useState } from 'react'
import { AGENTS, type AgentId } from '../agents'
import { askAgent, type Report, type RoomMessage } from '../api'
import { t } from '../i18n'
import { askLabel, speak } from '../voices'
import { Avatar } from './Avatar'

const ORDER: AgentId[] = ['raio', 'dieta', 'memo']

interface RoomProps {
  report: Report
  onChanged: () => void
}

/** The agents room of this project. Agents only speak when asked (a click here or /token-diet). */
export function Room({ report, onChanged }: RoomProps) {
  const [busy, setBusy] = useState<AgentId | null>(null)
  const [error, setError] = useState<string | null>(null)
  const end = useRef<HTMLDivElement>(null)
  const messages = report.room.messages
  useEffect(() => end.current?.scrollIntoView({ block: 'end' }), [messages.length])

  const ask = async (agent: AgentId) => {
    setBusy(agent)
    setError(null)
    try {
      await askAgent(agent)
      onChanged()
    } catch (failure: unknown) {
      setError(failure instanceof Error ? failure.message : String(failure))
    } finally {
      setBusy(null)
    }
  }

  if (!report.room.opted_in) {
    return (
      <main className="grade sala">
        <section className="cartao sala-fora">
          <h2>{t.roomOffTitle}</h2>
          <p>{t.roomOff}</p>
        </section>
      </main>
    )
  }

  return (
    <main className="grade sala">
      <section className="cartao membros">
        <h2>
          {t.members}
          <small>{t.membersHint}</small>
        </h2>
        <ul className="lista-membros">
          {ORDER.map((id) => (
            <li key={id}>
              <Avatar agent={AGENTS[id]} size={36} />
              <span className="agente-nome">
                <b>{AGENTS[id].name}</b>
                <span>{AGENTS[id].role}</span>
              </span>
              <button className="vidro-btn" disabled={busy !== null} onClick={() => void ask(id)}>
                {busy === id ? '…' : t.ask}
              </button>
            </li>
          ))}
        </ul>
        {error && <p className="nota failed">{error}</p>}
      </section>
      <section className="cartao conversa">
        <h2>
          {t.conversation}
          <small>{report.project_dir.split('/').pop()}</small>
        </h2>
        <div className="mensagens">
          {messages.length === 0 && <p className="vazio">{t.roomEmpty}</p>}
          {messages.map((message, index) => (
            <Bubble key={`${message.ms}-${index}`} message={message} />
          ))}
          <div ref={end} />
        </div>
        <div className="perguntas">
          {ORDER.map((id) => (
            <button key={id} className="vidro-btn" disabled={busy !== null} onClick={() => void ask(id)}>
              {askLabel(id)}
            </button>
          ))}
        </div>
      </section>
    </main>
  )
}

interface BubbleProps {
  message: RoomMessage
}

function Bubble({ message }: BubbleProps) {
  const time = new Date(message.ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  if (message.author === 'you') {
    return (
      <div className="msg minha">
        <span className="texto">{speak(message)}</span>
        <span className="hora">{time}</span>
      </div>
    )
  }
  const agent = AGENTS[message.author]
  return (
    <div className="msg">
      <Avatar agent={agent} size={28} />
      <div className="corpo">
        <span className="quem">
          <b style={{ color: agent.color }}>{agent.name}</b>
          <span className="hora">{time}</span>
        </span>
        <span className="texto">{speak(message)}</span>
      </div>
    </div>
  )
}
