import { useEffect, useRef, useState } from 'react'
import { type Bot, type BotUsage, botUsage } from '../../api'
import { AGENT_LABEL } from '../../avatars'
import type { Bots } from '../../bots'
import { t } from '../../i18n'
import { modelName } from '../BotSettings'

/** Claude Code's aliases point at the newest model of each family. */
const ALIAS_NAMES: Record<string, string> = { opus: 'Claude Opus', sonnet: 'Claude Sonnet', haiku: 'Claude Haiku', fable: 'Claude Fable' }

const nameOf = (model: string) => ALIAS_NAMES[model] ?? modelName(model)

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
  const box = useRef<HTMLDivElement>(null)

  useEffect(() => {
    setUsage(null)
    botUsage(bot.id).then(setUsage).catch(() => setUsage(null))
  }, [bot.id])
  useEffect(() => {
    if (!open) return undefined
    const away = (event: MouseEvent) => !box.current?.contains(event.target as Node) && setOpen(false)
    const esc = (event: KeyboardEvent) => event.key === 'Escape' && setOpen(false)
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
  const pick = (next: { model?: string | null; effort?: string | null }) =>
    void bots.act('configure', {
      id: bot.id,
      model: ('model' in next ? next.model : bot.model) ?? '',
      effort: ('effort' in next ? next.effort : bot.effort) ?? '',
      persona: null,
    })
  const needle = query.trim().toLowerCase()
  const options = (usage?.offered_models ?? []).filter((m) => !needle || `${m} ${nameOf(m)}`.toLowerCase().includes(needle))

  return (
    <div className="modelo-escolha" ref={box}>
      <button type="button" className="modelo-botao" aria-expanded={open} onClick={() => setOpen(!open)}>
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
          <input autoFocus value={query} placeholder={t.searchModels} onChange={(event) => setQuery(event.target.value)} />
          <ul role="radiogroup" aria-label={t.modelLabel}>
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
                    {nameOf(m)}
                    {isDefault && <span className="modelo-selo">{t.defaultOption.toLowerCase()}</span>}
                  </button>
                </li>
              )
            })}
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
            <footer>
              <span>{t.effortLabel}</span>
              <select value={bot.effort ?? ''} aria-label={t.effortLabel} onChange={(event) => pick({ effort: event.target.value || null })}>
                <option value="">{fallbackEffort ? `${t.defaultOption} · ${fallbackEffort}` : t.defaultOption}</option>
                {usage?.offered_efforts.map((e) => (
                  <option key={e} value={e}>
                    {e}
                  </option>
                ))}
              </select>
            </footer>
          )}
        </div>
      )}
    </div>
  )
}
