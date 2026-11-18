import { useEffect, useState } from 'react'
import { type Bot, type BotUsage, type Persona, botUsage } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import type { Bots } from '../bots'
import { t } from '../i18n'
import { type Look, AvatarEditor } from './AvatarEditor'
import { Bars } from './Bars'
import './settings.css'

const MAX_NAME = 32
const MAX_MANNER = 1200

/** "claude-opus-5-5" reads as "Opus 5.5"; other names stay as they are. */
export function modelName(model: string): string {
  const claude = model.match(/^claude-([a-z]+)-(\d+)-(\d+)/)
  if (!claude) return model
  return `${claude[1][0].toUpperCase()}${claude[1].slice(1)} ${claude[2]}.${claude[3]}`
}

interface BotSettingsProps {
  bot: Bot
  bots: Bots
  onClose: () => void
}

/** What the project runs on, the bot's model and effort, and its look. */
export function BotSettings({ bot, bots, onClose }: BotSettingsProps) {
  const [usage, setUsage] = useState<BotUsage | null>(null)
  const [failed, setFailed] = useState<string | null>(null)
  const face = faceForBot(bot)
  const [look, setLook] = useState<Look>({ shape: bot.persona?.shape ?? 'monitor', color: face.color, finish: bot.persona?.finish ?? 'solid' })
  const [name, setName] = useState(bot.persona?.name ?? AGENT_LABEL[bot.agent])
  const [manner, setManner] = useState(bot.persona?.instructions ?? '')

  useEffect(() => {
    let alive = true
    botUsage(bot.id)
      .then((next) => alive && setUsage(next))
      .catch((error: unknown) => alive && setFailed(error instanceof Error ? error.message : String(error)))
    return () => {
      alive = false
    }
  }, [bot.id])

  const run = (model: string | null, effort: string | null, persona: Persona | null) =>
    void bots.act('configure', { id: bot.id, model: model ?? '', effort: effort ?? '', persona })
  const persona: Persona = {
    preset: bot.persona?.preset ?? 'custom',
    name: name.trim() || AGENT_LABEL[bot.agent],
    instructions: manner,
    color: look.color,
    shape: look.shape,
    finish: look.finish,
  }
  const changed = JSON.stringify(persona) !== JSON.stringify(bot.persona)
  const days = usage?.days ?? 30
  const models = usage?.models ?? []
  const efforts = usage?.efforts ?? []

  return (
    <aside className="ajustes" aria-label={t.settings}>
      <header className="ajustes-topo">
        <b>{t.settings}</b>
        <button type="button" className="voltar" onClick={onClose}>
          {t.close}
        </button>
      </header>
      <div className="ajustes-corpo">
        <span className="ajustes-grupo">{t.lastDays.replace('{n}', String(days))}</span>
        {failed && <p className="nota failed">{failed}</p>}
        {usage && models.length === 0 && <p className="nota">{t.noUsageDays.replace('{n}', String(days))}</p>}
        {models.length > 0 && (
          <Bars title={t.modelsTitle} subtitle={t.modelsSub} color={face.color} rows={models.map((m) => ({ label: modelName(m.name), value: m.tokens }))} />
        )}
        {efforts.length > 0 && (
          <Bars title={t.effortTitle} subtitle={t.effortSub} color={face.color} rows={efforts.map((e) => ({ label: e.name, value: e.requests }))} />
        )}

        <span className="ajustes-grupo">{`${t.botUses} · ${AGENT_LABEL[bot.agent]}`}</span>
        {usage && (
          <>
            <Choice
              label={t.modelLabel}
              current={bot.model}
              fallback={usage.default.model}
              options={usage.offered_models}
              busy={bot.busy}
              show={modelName}
              onPick={(model) => run(model, bot.effort, null)}
            />
            {usage.offered_efforts.length > 0 && (
            <Choice
              label={t.effortLabel}
              current={bot.effort}
              fallback={usage.default.effort}
              options={usage.offered_efforts}
              busy={bot.busy}
              show={(e) => e}
              onPick={(effort) => run(bot.model, effort, null)}
            />
            )}
            <p className="ajustes-nota">{t.nextMessages}</p>
          </>
        )}

        <span className="ajustes-grupo">{t.appearance}</span>
        <AvatarEditor value={look} onChange={setLook} />
        <label className="ajustes-campo">
          <span>{t.nameLabel}</span>
          <input value={name} maxLength={MAX_NAME} onChange={(event) => setName(event.target.value)} />
        </label>
        <label className="ajustes-campo">
          <span>{t.mannerLabel}</span>
          <textarea rows={3} value={manner} maxLength={MAX_MANNER} placeholder={t.mannerHint} onChange={(event) => setManner(event.target.value)} />
        </label>
        <span className="acao">
          <button type="button" className="cheio" disabled={!changed || bot.busy} onClick={() => run(bot.model, bot.effort, persona)}>
            {t.save}
          </button>
        </span>
        {bots.error && <p className="nota failed quebra">{bots.error}</p>}
      </div>
    </aside>
  )
}

/** Past this many options, a field with suggestions replaces the grid. */
const GRID_LIMIT = 8

interface ChoiceProps {
  label: string
  current: string | null
  fallback: string | null
  options: string[]
  /** Type any value (with the options as suggestions) instead of picking from a grid. */
  free?: boolean
  busy: boolean
  show: (value: string) => string
  onPick: (value: string | null) => void
}

/** One grouped selector: the CLI's default first, then each option. */
export function Choice({ label, current, fallback, options, free = false, busy, show, onPick }: ChoiceProps) {
  if (free || options.length > GRID_LIMIT) return <LongChoice label={label} current={current} fallback={fallback} options={options} busy={busy} show={show} onPick={onPick} />
  return (
    <div className="ajustes-campo">
      <span>{label}</span>
      <div className="ajustes-grade" role="radiogroup" aria-label={label} style={{ gridTemplateColumns: `repeat(${options.length + 1}, minmax(0, 1fr))` }}>
        <button type="button" role="radio" aria-checked={current === null} className={current === null ? 'ativo' : ''} disabled={busy} onClick={() => onPick(null)}>
          <b>{t.defaultOption}</b>
          {fallback && <small>{show(fallback)}</small>}
        </button>
        {options.map((option) => (
          <button key={option} type="button" role="radio" aria-checked={current === option} className={current === option ? 'ativo' : ''} disabled={busy} onClick={() => onPick(option)}>
            <b>{show(option)}</b>
          </button>
        ))}
      </div>
    </div>
  )
}

/** Many models (Cursor lists hundreds): the default, or one typed with suggestions. */
function LongChoice({ label, current, fallback, options, busy, onPick }: ChoiceProps) {
  const [draft, setDraft] = useState(current ?? '')
  const listId = `modelos-${label}`
  return (
    <div className="ajustes-campo">
      <span>{label}</span>
      <div className="ajustes-livre">
        <button type="button" className={`vidro-btn ${current === null ? 'ativo' : ''}`} disabled={busy} onClick={() => onPick(null)}>
          {fallback ? `${t.defaultOption} · ${fallback}` : t.defaultOption}
        </button>
        <input list={listId} value={draft} placeholder={t.otherModel} onChange={(event) => setDraft(event.target.value)} />
        <datalist id={listId}>
          {options.map((option) => (
            <option key={option} value={option} />
          ))}
        </datalist>
        <button type="button" className="vidro-btn" disabled={busy || !draft.trim() || draft.trim() === current} onClick={() => onPick(draft.trim())}>
          {t.use}
        </button>
      </div>
    </div>
  )
}
