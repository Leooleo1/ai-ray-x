import { useEffect, useId, useRef, useState } from 'react'
import { type Bot, type BotUsage, type Persona, botUsage } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import type { Bots } from '../bots'
import { compact, t } from '../i18n'
import { type Look, AvatarEditor } from './AvatarEditor'
import { Bars } from './Bars'
import { EffortMeter } from './EffortMeter'
import { RoutinesEditor } from './RoutinesEditor'
import './settings.css'

const MAX_NAME = 32
const MAX_MANNER = 1200
const MAX_ROLE = 40

/** "claude-opus-5-5" reads as "Opus 5.5"; other names stay as they are. */
/** Claude Code's model aliases, as a person reads them. */
const ALIASES: Record<string, string> = { opus: 'Opus', sonnet: 'Sonnet', haiku: 'Haiku', fable: 'Fable' }

export function modelName(model: string): string {
  if (ALIASES[model]) return ALIASES[model]
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
  const [role, setRole] = useState(bot.persona?.role ?? '')
  const [confirmClose, setConfirmClose] = useState(false)

  useEffect(() => {
    let alive = true
    botUsage(bot.id)
      .then((next) => alive && setUsage(next))
      .catch((error: unknown) => alive && setFailed(error instanceof Error ? error.message : String(error)))
    return () => {
      alive = false
    }
  }, [bot.id, bot.agent])

  const run = (model: string | null, effort: string | null, persona: Persona | null) =>
    void bots.act('configure', { id: bot.id, model: model ?? '', effort: effort ?? '', persona })
  const persona: Persona = {
    preset: bot.persona?.preset ?? 'custom',
    name: name.trim() || AGENT_LABEL[bot.agent],
    instructions: manner,
    color: look.color,
    shape: look.shape,
    finish: look.finish,
    role: role.trim(),
  }
  const changed = JSON.stringify(persona) !== JSON.stringify(bot.persona)
  useEffect(() => {
    if (!changed) setConfirmClose(false)
  }, [changed])
  const tryClose = () => {
    if (changed && !confirmClose) {
      setConfirmClose(true)
      return
    }
    onClose()
  }
  const resetPersona = () => {
    setLook({ shape: bot.persona?.shape ?? 'monitor', color: face.color, finish: bot.persona?.finish ?? 'solid' })
    setName(bot.persona?.name ?? AGENT_LABEL[bot.agent])
    setManner(bot.persona?.instructions ?? '')
    setRole(bot.persona?.role ?? '')
    setConfirmClose(false)
  }
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      const target = event.target as HTMLElement | null
      // Let inner popovers (like LongChoice) consume Escape first.
      if (target?.closest('.ajustes-combo.aberto')) return
      if (changed && !confirmClose) setConfirmClose(true)
      else onClose()
    }
    // A click outside the drawer closes it as Escape does (asking first when a change is unsaved);
    // the button that opens it toggles it by itself.
    const onOutside = (event: MouseEvent) => {
      const target = event.target as HTMLElement | null
      if (!target || target.closest('.ajustes, [data-abre-ajustes]')) return
      if (changed && !confirmClose) setConfirmClose(true)
      else onClose()
    }
    document.addEventListener('keydown', onKey)
    document.addEventListener('mousedown', onOutside)
    return () => {
      document.removeEventListener('keydown', onKey)
      document.removeEventListener('mousedown', onOutside)
    }
  }, [changed, confirmClose, onClose])
  const days = usage?.days ?? 30
  const models = usage?.models ?? []
  const partners = bots.state.bots.filter((b) => b.dir === bot.dir && b.id !== bot.id)
  const efforts = usage?.efforts ?? []

  return (
    <aside className="ajustes" aria-label={t.settings}>
      <header className="ajustes-topo">
        <b>{t.settings}</b>
        <button type="button" className={`voltar ${confirmClose ? 'confirmar' : ''}`} onBlur={() => setConfirmClose(false)} onClick={tryClose}>
          {confirmClose ? t.closeDiscard : t.close}
        </button>
      </header>
      <div className="ajustes-corpo">
        <span className="ajustes-grupo">{t.lastDays.replace('{n}', String(days))}</span>
        {failed && <p className="nota failed">{failed}</p>}
        {usage && models.length === 0 && <p className="nota">{t.noUsageDays.replace('{n}', String(days))}</p>}
        {models.length > 0 && (
          <Bars title={t.modelsTitle} subtitle={t.modelsSub} color={face.color} rows={models.map((m) => ({ label: m.name === 'default' ? (usage?.default.model ? modelName(usage.default.model) : t.defaultOption) : modelName(m.name), value: m.tokens }))} />
        )}
        {efforts.length > 0 && (
          <Bars title={t.effortTitle} subtitle={t.effortSub} color={face.color} rows={efforts.map((e) => ({ label: e.name === 'default' ? (usage?.default.effort ?? t.defaultOption) : e.name, value: e.requests }))} />
        )}

        <span className="ajustes-grupo">{`${t.botUses} · ${AGENT_LABEL[bot.agent]}`}</span>
        {usage && (
          <>
            <Choice
              label={t.modelLabel}
              current={bot.model}
              fallback={usage.default.model}
              options={usage.offered_models}
              show={modelName}
              onPick={(model) => run(model, bot.effort, null)}
            />
            {usage.offered_efforts.length > 0 && (
            <EffortMeter
              label={t.effortLabel}
              current={bot.effort}
              fallback={usage.default.effort}
              options={usage.offered_efforts}
              color={face.color}
              onPick={(effort) => run(bot.model, effort, null)}
            />
            )}
            <Choice
              label={t.talkModelLabel}
              current={bot.talk_model ?? null}
              fallback={bot.model ?? usage.default.model}
              options={usage.offered_models}
              show={modelName}
              nullLabel={t.sameModel}
              onPick={(model) => void bots.act('talk_model', { id: bot.id, model: model ?? '' })}
            />
            <p className="ajustes-nota">{`${t.talkModelNote}; ${t.nextMessages}`}</p>
          </>
        )}

        {partners.length > 0 && (
          <>
            <span className="ajustes-grupo">{t.reviewerLabel}</span>
            <Choice
              label={t.reviewerLabel}
              bare
              current={bot.reviewer ?? null}
              fallback={null}
              options={partners.map((p) => p.id)}
              show={(id) => AGENT_LABEL[partners.find((p) => p.id === id)?.agent ?? bot.agent]}
              nullLabel={t.noReviewer}
              onPick={(reviewer) => void bots.act('reviewer', { id: bot.id, reviewer: reviewer ?? '' })}
            />
            <p className="ajustes-nota">{t.reviewerNote}</p>
          </>
        )}

        {bot.budget && (
          <>
            <span className="ajustes-grupo">{t.dailyLimit}</span>
            <Choice
              label={t.dailyLimit}
              current={bot.budget.limit === null ? null : String(bot.budget.limit)}
              fallback={null}
              options={bot.budget.choices.map(String)}
              show={(tokens) => compact(Number(tokens))}
              nullLabel={t.noLimit}
              onPick={(tokens) => void bots.act('limit', { id: bot.id, tokens: tokens === null ? 0 : Number(tokens) })}
            />
            <p className="ajustes-nota">
              {bot.budget.limit === null
                ? t.usedToday.replace('{used}', compact(bot.budget.used))
                : t.usedOfLimit.replace('{used}', compact(bot.budget.used)).replace('{limit}', compact(bot.budget.limit))}
            </p>
          </>
        )}

        {bot.grants && <GrantsBlock bot={bot} bots={bots} />}

        <span className="ajustes-grupo">{t.allowedCommands}</span>
        {(bot.allowed ?? []).length === 0 ? (
          <p className="ajustes-nota">{t.allowedNone}</p>
        ) : (
          <ul className="liberados">
            {(bot.allowed ?? []).map((command) => (
              <li key={command}>
                <code>{command}</code>
                <button type="button" onClick={() => void bots.act('disallow', { id: bot.id, command })}>
                  {t.disallow}
                </button>
              </li>
            ))}
          </ul>
        )}

        <span className="ajustes-grupo">{t.routines}</span>
        <RoutinesEditor bot={bot} bots={bots} />

        <span className="ajustes-grupo">{t.appearance}</span>
        <AvatarEditor value={look} onChange={setLook} />
        <label className="ajustes-campo">
          <span>{t.nameLabel}</span>
          <input value={name} maxLength={MAX_NAME} onChange={(event) => setName(event.target.value)} />
        </label>
        <label className="ajustes-campo">
          <span>{t.roleLabel}</span>
          <input value={role} maxLength={MAX_ROLE} placeholder={t.roleHint} onChange={(event) => setRole(event.target.value)} />
        </label>
        <label className="ajustes-campo">
          <span>{t.mannerLabel}</span>
          <textarea rows={3} value={manner} maxLength={MAX_MANNER} placeholder={t.mannerHint} onChange={(event) => setManner(event.target.value)} />
        </label>
        <span className="acao">
          <button type="button" className="cheio" disabled={!changed} onClick={() => run(bot.model, bot.effort, persona)}>
            {t.save}
          </button>
        </span>
        {bots.error && <p className="nota failed quebra">{bots.error}</p>}
      </div>
      {changed && (
        <footer className="ajustes-rodape">
          <span className="ajustes-pendente">
            <i className="ponto alerta" aria-hidden="true" />
            {t.unsavedChanges}
          </span>
          <button type="button" className="link" onClick={resetPersona}>
            {t.cancel}
          </button>
          <button type="button" className="cheio" onClick={() => run(bot.model, bot.effort, persona)}>
            {t.save}
          </button>
        </footer>
      )}
    </aside>
  )
}

/** Past this many options, a searchable dropdown replaces the grid. */
const GRID_LIMIT = 6

interface ChoiceProps {
  label: string
  current: string | null
  fallback: string | null
  options: string[]
  /** Type any value (with the options as suggestions) instead of picking from a grid. */
  free?: boolean
  show: (value: string) => string
  onPick: (value: string | null) => void
  /** What the first option says; the CLI's default unless given. */
  nullLabel?: string
  /** Under a section title that already names it: no label of its own (screen readers still get it). */
  bare?: boolean
}

/** One grouped selector: the CLI's default first, then each option. */
export function Choice({ label, current, fallback, options, free = false, show, onPick, nullLabel = t.defaultOption, bare = false }: ChoiceProps) {
  if (free || options.length > GRID_LIMIT) {
    return <LongChoice label={label} current={current} fallback={fallback} options={options} show={show} onPick={onPick} nullLabel={nullLabel} />
  }
  // One line of equal cells, never wrapped (longer lists use the searchable picker).
  const gridStyle = { gridTemplateColumns: `repeat(${options.length + 1}, minmax(0, 1fr))` }
  return (
    <div className="ajustes-campo">
      {!bare && <span>{label}</span>}
      <div className="ajustes-grade" role="radiogroup" aria-label={label} style={gridStyle}>
        <button type="button" role="radio" aria-checked={current === null} className={current === null ? 'ativo' : ''} onClick={() => onPick(null)}>
          <b>{nullLabel}</b>
          {fallback && <small>{show(fallback)}</small>}
        </button>
        {options.map((option) => (
          <button key={option} type="button" role="radio" aria-checked={current === option} className={current === option ? 'ativo' : ''} onClick={() => onPick(option)}>
            <b>{show(option)}</b>
          </button>
        ))}
      </div>
    </div>
  )
}

/** Many models (Cursor lists hundreds): searchable popover with custom value support. */
function LongChoice({ label, current, fallback, options, show, onPick, nullLabel = t.defaultOption }: ChoiceProps) {
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState(current ?? '')
  const box = useRef<HTMLDivElement>(null)
  const inputId = useId()
  useEffect(() => {
    setDraft(current ?? '')
  }, [current])
  useEffect(() => {
    if (!open) return undefined
    const away = (event: MouseEvent) => {
      if (!box.current?.contains(event.target as Node)) setOpen(false)
    }
    const esc = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        setOpen(false)
      }
    }
    document.addEventListener('mousedown', away)
    document.addEventListener('keydown', esc)
    return () => {
      document.removeEventListener('mousedown', away)
      document.removeEventListener('keydown', esc)
    }
  }, [open])
  const needle = draft.trim().toLowerCase()
  const matched = options.filter((o) => !needle || `${o} ${show(o)}`.toLowerCase().includes(needle))
  const choose = (next: string | null) => {
    setDraft(next ?? '')
    setOpen(false)
    if (next !== current) onPick(next)
  }
  const applyTyped = () => {
    const next = draft.trim() || null
    setOpen(false)
    if (next !== current) onPick(next)
  }
  return (
    <div className={`ajustes-campo ajustes-combo ${open ? 'aberto' : ''}`} ref={box}>
      <label htmlFor={inputId}>{label}</label>
      <div className="ajustes-combo-wrap">
        <input
          id={inputId}
          className="ajustes-modelo"
          value={draft}
          placeholder={fallback ? `${nullLabel} · ${show(fallback)}` : nullLabel}
          onFocus={() => setOpen(true)}
          onChange={(event) => {
            setDraft(event.target.value)
            setOpen(true)
          }}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault()
              applyTyped()
            }
          }}
        />
        {draft && (
          <button
            type="button"
            className="campo-limpar"
            aria-label={t.clearSearch}
            title={t.clearSearch}
            onClick={() => choose(null)}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        )}
      </div>
      {open && (
        <ul className="ajustes-combo-lista" role="listbox" aria-label={label}>
          <li role="option" aria-selected={current === null}>
            <button type="button" className={current === null ? 'ativo' : ''} onClick={() => choose(null)}>
              <span>{nullLabel}</span>
              {fallback && <small>{show(fallback)}</small>}
            </button>
          </li>
          {matched.slice(0, 60).map((option) => (
            <li key={option} role="option" aria-selected={current === option}>
              <button type="button" className={current === option ? 'ativo' : ''} onClick={() => choose(option)}>
                <span>{show(option)}</span>
                {show(option) !== option && <small>{option}</small>}
              </button>
            </li>
          ))}
          {needle && !options.includes(draft.trim()) && (
            <li role="option" aria-selected={false}>
              <button type="button" onClick={applyTyped}>
                <span>{`${t.use} “${draft.trim()}”`}</span>
              </button>
            </li>
          )}
        </ul>
      )}
    </div>
  )
}

/** What the bot may do beyond reading its project: the phone, the web, MCP servers, a weekly ceiling. */
function GrantsBlock({ bot, bots }: { bot: Bot; bots: Bots }) {
  const grants = bot.grants
  if (!grants) return null
  const set = (change: Record<string, unknown>) => void bots.act('grants', { id: bot.id, ...change })
  const rows: { key: 'devices' | 'web' | 'mcp' | 'auto_edit' | 'auto_run'; label: string; note: string; shown: boolean }[] = [
    { key: 'auto_run', label: t.grantAutoRun, note: t.grantAutoRunNote, shown: true },
    { key: 'auto_edit', label: t.grantAutoEdit, note: t.grantAutoEditNote, shown: true },
    { key: 'devices', label: t.grantDevices, note: t.grantDevicesNote, shown: true },
    { key: 'web', label: t.grantWeb, note: t.grantWebNote, shown: grants.web_possible },
    { key: 'mcp', label: t.grantMcp, note: t.grantMcpNote, shown: grants.mcp_possible },
  ]
  return (
    <>
      <span className="ajustes-grupo">{t.grantsTitle}</span>
      {rows
        .filter((row) => row.shown)
        .map((row) => (
          <label key={row.key} className="permissao">
            <button type="button" className={`config-chave ${grants[row.key] ? 'ligada' : ''}`} role="switch" aria-checked={grants[row.key]} onClick={() => set({ [row.key]: !grants[row.key] })}>
              <i />
            </button>
            <span>
              <b>{row.label}</b>
              <small className={(row.key === 'mcp' || row.key === 'auto_edit' || row.key === 'auto_run') && grants[row.key] ? 'permissao-alerta' : ''}>{row.note}</small>
            </span>
          </label>
        ))}
      <Choice
        label={t.weekLimit}
        current={grants.week_limit === null ? null : String(grants.week_limit)}
        fallback={null}
        options={grants.week_choices.map(String)}
        show={(tokens) => compact(Number(tokens))}
        nullLabel={t.noLimit}
        onPick={(tokens) => set({ week_limit: tokens === null ? 0 : Number(tokens) })}
      />
      <p className="ajustes-nota">
        {grants.week_limit === null
          ? t.usedThisWeek.replace('{used}', compact(grants.week_used))
          : t.usedOfWeek.replace('{used}', compact(grants.week_used)).replace('{limit}', compact(grants.week_limit))}
      </p>
    </>
  )
}
