import { type FormEvent, type KeyboardEvent, useEffect, useRef, useState } from 'react'
import { type BotAgent, type OtherAgent, freeAgents } from '../api'
import { AGENT_SHORT } from '../avatars'
import type { Bots } from '../bots'
import { t } from '../i18n'
import { AgentLogo } from './AgentLogo'
import { modelName } from './BotSettings'
import './free-chat.css'

const ALIAS_NAMES: Record<string, string> = {
  opus: 'Claude Opus',
  sonnet: 'Claude Sonnet',
  haiku: 'Claude Haiku',
  fable: 'Claude Fable',
}

const labelForModel = (model: string) => ALIAS_NAMES[model] ?? modelName(model)

interface FreeChatNewProps {
  bots: Bots
  onOpened: (botId: string, threadId: string) => void
}

/** Standalone free-chat start screen: pick any installed AI and model, write a question, and jump straight into the thread. */
export function FreeChatNew({ bots, onOpened }: FreeChatNewProps) {
  const [installed, setInstalled] = useState<OtherAgent[] | null>(null)
  const [agent, setAgent] = useState<BotAgent | null>(null)
  const [model, setModel] = useState('')
  const [text, setText] = useState('')
  const [sending, setSending] = useState(false)
  const inputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    let active = true
    freeAgents()
      .then((res) => {
        if (!active) return
        setInstalled(res.agents)
        setAgent((prev) => {
          if (prev && res.agents.some((a) => a.agent === prev)) return prev
          return (res.agents[0]?.agent as BotAgent | undefined) ?? null
        })
      })
      .catch(() => {
        if (!active) return
        const fallback: OtherAgent[] = bots.state.agents.map((a) => ({ agent: a.id, name: a.name, models: [] }))
        setInstalled(fallback)
        setAgent((prev) => prev ?? (fallback[0]?.agent as BotAgent | undefined) ?? null)
      })
    return () => {
      active = false
    }
  }, [bots.state.agents])

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  const list = installed ?? bots.state.agents.map((a) => ({ agent: a.id, name: a.name, models: [] }))
  const current = list.find((a) => a.agent === agent) ?? list[0] ?? null
  const activeAgent = (current?.agent as BotAgent | undefined) ?? null
  const models = current?.models ?? []

  const pickAgent = (next: BotAgent) => {
    setAgent(next)
    setModel('')
  }

  const submit = async () => {
    const trimmed = text.trim()
    if (!trimmed || !activeAgent || sending) return
    setSending(true)
    try {
      const next = await bots.act('free_ask', { agent: activeAgent, model, text: trimmed })
      if (next?.opened) {
        setText('')
        onOpened(next.opened.bot, next.opened.thread)
      }
    } finally {
      setSending(false)
    }
  }

  const onFormSubmit = (event: FormEvent) => {
    event.preventDefault()
    void submit()
  }

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      void submit()
    }
  }

  return (
    <main className="chat">
      <section className="cartao livre-novo-pagina">
        <div className="livre-novo-centro">
          <h2 className="livre-novo-titulo">{t.freeChatTitle}</h2>

          {installed !== null && list.length === 0 ? (
            <p className="nota">{t.freeChatNoClis}</p>
          ) : (
            <>
              <div className="livre-controles">
                <div className="livre-ias" role="radiogroup" aria-label={t.freeChatChooseAi}>
                  {list.map((item) => {
                    const id = item.agent as BotAgent
                    const chosen = activeAgent === id
                    return (
                      <button
                        key={id}
                        type="button"
                        role="radio"
                        aria-checked={chosen}
                        className={`livre-ia-btn ${chosen ? 'ativo' : ''}`}
                        onClick={() => pickAgent(id)}
                      >
                        <AgentLogo agent={id} size={15} />
                        <span>{AGENT_SHORT[id] ?? item.name}</span>
                      </button>
                    )
                  })}
                </div>
                {activeAgent && (
                  <select
                    className="livre-modelo-select"
                    aria-label={t.freeChatChooseModel}
                    value={model}
                    disabled={installed === null && models.length === 0}
                    onChange={(event) => setModel(event.target.value)}
                  >
                    <option value="">{installed === null ? t.loadingModels : t.defaultOption}</option>
                    {models.map((m) => (
                      <option key={m} value={m}>
                        {labelForModel(m)}
                      </option>
                    ))}
                  </select>
                )}
              </div>

              <form className="livre-campo" onSubmit={onFormSubmit}>
                <textarea
                  ref={inputRef}
                  rows={4}
                  value={text}
                  maxLength={bots.state.max_text}
                  placeholder={t.freeChatPlaceholder}
                  aria-label={t.freeChatPlaceholder}
                  disabled={sending}
                  onChange={(event) => setText(event.target.value)}
                  onKeyDown={onKeyDown}
                />
                <div className="livre-campo-pe">
                  <span className="livre-selo">{t.freeChatReadOnly}</span>
                  <button type="submit" className="livre-enviar" disabled={sending || !text.trim() || !activeAgent}>
                    {t.send}
                  </button>
                </div>
              </form>
            </>
          )}
          {bots.error && <p className="nota failed">{bots.error}</p>}
        </div>
      </section>
    </main>
  )
}
