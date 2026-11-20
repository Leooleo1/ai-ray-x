import { useEffect, useRef, useState } from 'react'
import { type Bot, type BotUsage, type OtherAgent, botUsage, otherAgents } from '../../api'
import { AGENT_LABEL } from '../../avatars'
import type { Bots } from '../../bots'
import { t } from '../../i18n'
import { modelName } from '../BotSettings'

/** Claude Code's aliases point at the newest model of each family. */
const ALIAS_NAMES: Record<string, string> = { opus: 'Claude Opus', sonnet: 'Claude Sonnet', haiku: 'Claude Haiku', fable: 'Claude Fable' }

const nameOf = (model: string) => ALIAS_NAMES[model] ?? modelName(model)

/** Names for a list of models: the friendly one, or the id itself when two would read the same. */
function namesFor(models: string[]): (model: string) => string {
  const counts = new Map<string, number>()
  for (const m of models) counts.set(nameOf(m), (counts.get(nameOf(m)) ?? 0) + 1)
  return (m) => ((counts.get(nameOf(m)) ?? 0) > 1 ? m : nameOf(m))
}

interface ModelPickerProps {
  bot: Bot
  bots: Bots
  /** Changes when a card asks to open the menu. */
  openSignal?: number
}

/** The model and effort in the header; a click opens the list with search and the effort below. */
export function ModelPicker({ bot, bots, openSignal = 0 }: ModelPickerProps) {
  const [open, setOpen] = useState(false)
  useEffect(() => {
    if (openSignal > 0) setOpen(true)
  }, [openSignal])
  const [usage, setUsage] = useState<BotUsage | null>(null)
  const [query, setQuery] = useState('')
  // The other AIs: asked once the menu opens (each CLI lists its models, which takes a moment).
  const [others, setOthers] = useState<OtherAgent[] | null>(null)
  const box = useRef<HTMLDivElement>(null)
  useEffect(() => setOthers(null), [bot.id, bot.agent])
  useEffect(() => {
    if (!open || others !== null) return
    otherAgents(bot.id)
      .then((found) => setOthers(found.agents))
      .catch(() => setOthers([]))
  }, [open, others, bot.id])
  const close = () => {
    setOpen(false)
    setQuery('')
  }
  const switchTo = (agent: string, next: string | null) => {
    close()
    void bots.act('switch_agent', { id: bot.id, agent, model: next ?? '' })
  }

  useEffect(() => {
    setUsage(null)
    botUsage(bot.id).then(setUsage).catch(() => setUsage(null))
  }, [bot.id, bot.agent])
  useEffect(() => {
    if (!open) return undefined
    const away = (event: MouseEvent) => !box.current?.contains(event.target as Node) && close()
    const esc = (event: KeyboardEvent) => event.key === 'Escape' && close()
    document.addEventListener('mousedown', away)
    document.addEventListener('keydown', esc)
    return () => {
      document.removeEventListener('mousedown', away)
      document.removeEventListener('keydown', esc)
    }
  }, [open])

  const fallbackModel = usage?.default.model ?? null
  const fallbackEffort = usage?.default.effort ?? null
  const model = bot.model ?? fallbackModel
  const effort = bot.effort ?? fallbackEffort
  const pick = (next: { model?: string | null; effort?: string | null }, keepOpen = false) => {
    if (!keepOpen) close()
    void bots.act('configure', {
      id: bot.id,
      model: ('model' in next ? next.model : bot.model) ?? '',
      effort: ('effort' in next ? next.effort : bot.effort) ?? '',
      persona: null,
    })
  }
  const needle = query.trim().toLowerCase()
  const options = (usage?.offered_models ?? []).filter((m) => !needle || `${m} ${nameOf(m)}`.toLowerCase().includes(needle))
  const label = namesFor(usage?.offered_models ?? [])
  const otherMatches =
    others?.some((other) => other.name.toLowerCase().includes(needle) || other.models.some((m) => `${m} ${nameOf(m)} ${other.name}`.toLowerCase().includes(needle))) ?? false

  return (
    <div className="modelo-escolha" ref={box}>
      <button type="button" className="modelo-botao" aria-expanded={open} onClick={() => (open ? close() : setOpen(true))}>
        <b>{model ? nameOf(model) : AGENT_LABEL[bot.agent]}</b>
        {effort && <span>{`· ${effort}`}</span>}
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d={open ? 'M6 15l6-6 6 6' : 'M6 9l6 6 6-6'} />
        </svg>
      </button>
      {open && (
        <div className="modelo-menu" role="dialog" aria-label={t.modelLabel}>
          <header>
            <b>{AGENT_LABEL[bot.agent]}</b>
            <small>{t.nextMessages}</small>
          </header>
          <div className="busca-wrap">
            <input autoFocus value={query} placeholder={t.searchModels} aria-label={t.searchModels} onChange={(event) => setQuery(event.target.value)} />
            {query && (
              <button type="button" className="campo-limpar" aria-label={t.clearSearch} title={t.clearSearch} onClick={() => setQuery('')}>
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            )}
          </div>
          <ul role="radiogroup" aria-label={t.modelLabel}>
            {usage === null && <li className="modelo-grupo">{t.loadingModels}</li>}
            {!needle && fallbackModel && !options.includes(fallbackModel) && (
              <li>
                <button type="button" role="radio" aria-checked={bot.model === null} onClick={() => pick({ model: null })}>
                  <i className={bot.model === null ? 'marcado' : ''} />
                  {nameOf(fallbackModel)}
                  <span className="modelo-selo">{t.defaultOption.toLowerCase()}</span>
                </button>
              </li>
            )}
            {options.map((m) => {
              const isDefault = m === fallbackModel
              const chosen = bot.model === m || (bot.model === null && isDefault)
              return (
                <li key={m}>
                  <button type="button" role="radio" aria-checked={chosen} onClick={() => pick({ model: isDefault ? null : m })}>
                    <i className={chosen ? 'marcado' : ''} />
                    {label(m)}
                    {isDefault && <span className="modelo-selo">{t.defaultOption.toLowerCase()}</span>}
                  </button>
                </li>
              )
            })}
            {others === null && <li className="modelo-grupo">{t.otherAisLoading}</li>}
            {others?.map((other) => {
              const shown = other.models.filter((m) => !needle || `${m} ${nameOf(m)} ${other.name}`.toLowerCase().includes(needle))
              const otherLabel = namesFor(other.models)
              if (needle && shown.length === 0 && !other.name.toLowerCase().includes(needle)) return null
              return [
                <li key={`${other.agent}-titulo`} className="modelo-grupo">
                  {other.name}
                </li>,
                <li key={`${other.agent}-padrao`}>
                  <button type="button" onClick={() => switchTo(other.agent, null)}>
                    <i />
                    {other.name}
                    <span className="modelo-selo">{t.defaultOption.toLowerCase()}</span>
                  </button>
                </li>,
                ...shown.map((m) => (
                  <li key={`${other.agent}-${m}`}>
                    <button type="button" onClick={() => switchTo(other.agent, m)}>
                      <i />
                      {otherLabel(m)}
                    </button>
                  </li>
                )),
              ]
            })}
            {needle && options.length === 0 && !otherMatches && <li className="modelo-grupo">{t.noMatchingModels}</li>}
            {needle && !options.includes(query.trim()) && (
              <li>
                <button type="button" onClick={() => pick({ model: query.trim() })}>
                  <i />
                  {`${t.use} “${query.trim()}”`}
                </button>
              </li>
            )}
          </ul>
          {(usage?.offered_efforts.length ?? 0) > 0 && (
            <footer className="modelo-esforco">
              <span>{t.effortLabel}</span>
              {/* The default level is one of the pills (marked); picking it goes back to the default. */}
              <div className="modelo-esforco-opcoes" role="radiogroup" aria-label={t.effortLabel}>
                {!fallbackEffort && (
                  <button type="button" role="radio" aria-checked={bot.effort === null} className={bot.effort === null ? 'ativo' : ''} onClick={() => pick({ effort: null }, true)}>
                    {t.defaultOption}
                  </button>
                )}
                {usage?.offered_efforts.map((e) => {
                  const isDefault = e === fallbackEffort
                  const chosen = bot.effort === e || (bot.effort === null && isDefault)
                  return (
                    <button
                      key={e}
                      type="button"
                      role="radio"
                      aria-checked={chosen}
                      className={`${chosen ? 'ativo' : ''} ${isDefault ? 'padrao' : ''}`}
                      title={isDefault ? t.defaultOption : undefined}
                      onClick={() => pick({ effort: isDefault ? null : e }, true)}
                    >
                      {e}
                    </button>
                  )
                })}
              </div>
            </footer>
          )}
        </div>
      )}
    </div>
  )
}
