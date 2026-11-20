import { useEffect, useState } from 'react'
import { IS_REMOTE, type Bot, type BotUsage, type WikiEntry, botUsage } from '../../api'
import type { Bots } from '../../bots'
import { compact, locale, percent, t } from '../../i18n'
import { modelName } from '../BotSettings'
import { SelectChip } from '../SelectChip'

const CHARS_PER_TOKEN = 3.6

interface LeversCardProps {
  targetName: string
  targetBot: Bot | undefined
  bots: Bots
  wikiPages: WikiEntry[] | null
  auditing: boolean
  onRunAudit: () => void
  onOpenAuditThread: (threadId: string) => void
  onOpenWiki: () => void
  onOpenBoard: () => void
  onAddBot: () => void
}

/** Suggests a clearly lighter model from the CLI's offered list when the bot runs a flagship model. */
function lighterSuggestion(main: string | null, offered: string[]): string | null {
  if (!offered.length) return null
  const lower = (main ?? '').toLowerCase()
  const pick = (needle: string) => offered.find((m) => m !== main && m.toLowerCase().includes(needle)) ?? null
  if (lower.includes('opus')) return pick('sonnet') ?? pick('haiku')
  if (lower.includes('sonnet')) return pick('haiku')
  if (lower.includes('pro') || lower.includes('codex') || lower.includes('gpt-5') || lower.includes('o3')) {
    return pick('mini') ?? pick('flash') ?? pick('haiku')
  }
  return pick('haiku') ?? pick('mini') ?? pick('flash')
}

/** Bento 4: 2x2 quick-action levers operating directly on the selected project's Bot and shared Wiki. */
export function LeversCard({
  targetName,
  targetBot,
  bots,
  wikiPages,
  auditing,
  onRunAudit,
  onOpenAuditThread,
  onOpenWiki,
  onOpenBoard,
  onAddBot,
}: LeversCardProps) {
  const [usage, setUsage] = useState<BotUsage | null>(null)
  const [busyKey, setBusyKey] = useState<string | null>(null)

  useEffect(() => {
    let alive = true
    setUsage(null)
    if (!targetBot) return undefined
    botUsage(targetBot.id)
      .then((data) => alive && setUsage(data))
      .catch(() => alive && setUsage(null))
    return () => {
      alive = false
    }
  }, [targetBot?.id])

  const alwaysPages = (wikiPages ?? []).filter((p) => p.always)
  const alwaysTokens = Math.round(alwaysPages.reduce((sum, p) => sum + p.chars, 0) / CHARS_PER_TOKEN)

  if (!targetBot) {
    return (
      <section className="cartao efi-alavancas">
        <header className="efi-card-topo">
          <h2>
            {t.efiLeversTitle}
            <small>{t.efiLeversHint.replace('{p}', targetName)}</small>
          </h2>
        </header>
        <div className="efi-sem-bot">
          <h3>{t.efiLeversNoBotTitle.replace('{p}', targetName)}</h3>
          <p>{t.efiLeversNoBotHint}</p>
          <div className="efi-alavanca-acoes">
            <button type="button" className="efi-pill-cta primario" onClick={onAddBot}>
              {t.efiLeversEnableBot}
            </button>
            <button type="button" className="efi-pill-cta" onClick={onOpenWiki}>
              {t.efiCtaOpenWiki}
            </button>
          </div>
        </div>
      </section>
    )
  }

  const mainModel = targetBot.model ?? usage?.default.model ?? null
  const offered = usage?.offered_models ?? []
  const lighter = lighterSuggestion(mainModel, offered)
  const talkModel = targetBot.talk_model ?? null

  const setTalkModel = async (model: string | null) => {
    setBusyKey('talk')
    try {
      await bots.act('talk_model', { id: targetBot.id, model: model ?? '' })
    } finally {
      setBusyKey(null)
    }
  }

  const setDailyLimit = async (tokens: number) => {
    setBusyKey('limit')
    try {
      await bots.act('limit', { id: targetBot.id, tokens })
    } finally {
      setBusyKey(null)
    }
  }

  const week = targetBot.week
  const weekDelta = week && week.last > 0 ? (week.this - week.last) / week.last : null
  const budget = targetBot.budget
  const budgetFill = budget && budget.limit ? Math.min(1, budget.used / budget.limit) : 0

  const auditor = targetBot.auditor
  const auditorThread = auditor?.thread && targetBot.threads.some((th) => th.id === auditor.thread) ? auditor.thread : null
  const auditorLast = auditor?.last_ms
    ? new Date(auditor.last_ms).toLocaleDateString(locale, { day: '2-digit', month: 'short' })
    : t.neverRan
  const canEnableAuditor = !IS_REMOTE

  return (
    <section className="cartao efi-alavancas">
      <header className="efi-card-topo">
        <h2>
          {t.efiLeversTitle}
          <small>{t.efiLeversHint.replace('{p}', targetBot.name)}</small>
        </h2>
      </header>

      <div className="efi-alavancas-grade">
        {/* 1. Lighter talk model */}
        <article className="efi-alavanca">
          <div className="efi-alavanca-topo">
            <h3>{t.efiLeverTalkTitle}</h3>
            {talkModel && <span className="efi-tag ok">{modelName(talkModel)}</span>}
          </div>
          <p className="efi-alavanca-texto">
            {talkModel
              ? t.efiLeverTalkActive.replace('{talk}', modelName(talkModel)).replace('{main}', mainModel ? modelName(mainModel) : t.cliDefault)
              : t.efiLeverTalkSame.replace('{main}', mainModel ? modelName(mainModel) : t.cliDefault)}
          </p>
          <div className="efi-alavanca-acoes">
            {!talkModel && lighter && (
              <button
                type="button"
                className="efi-pill-cta primario"
                disabled={busyKey === 'talk'}
                onClick={() => void setTalkModel(lighter)}
              >
                {t.efiLeverTalkQuick.replace('{model}', modelName(lighter))}
              </button>
            )}
            {offered.length > 0 && (
              <SelectChip
                label={t.talkModelLabel}
                value={talkModel ?? ''}
                options={[{ value: '', label: t.sameModel }, ...offered.map((m) => ({ value: m, label: modelName(m) }))]}
                onChange={(v) => void setTalkModel(v || null)}
              />
            )}
            {talkModel && (
              <button type="button" className="efi-pill-cta suave" disabled={busyKey === 'talk'} onClick={() => void setTalkModel(null)}>
                {t.efiLeverTalkReset}
              </button>
            )}
          </div>
        </article>

        {/* 2. Daily ceiling & weekly pace */}
        <article className="efi-alavanca">
          <div className="efi-alavanca-topo">
            <h3>{t.efiLeverBudgetTitle}</h3>
            {weekDelta !== null && (
              <span className={`efi-tag ${weekDelta > 0.15 ? 'brasa' : weekDelta < -0.05 ? 'ok' : ''}`}>
                {weekDelta >= 0
                  ? t.efiLeverWeekDeltaUp.replace('{p}', percent(weekDelta))
                  : t.efiLeverWeekDeltaDown.replace('{p}', percent(weekDelta))}
              </span>
            )}
          </div>
          <p className="efi-alavanca-texto">
            {budget
              ? budget.limit === null
                ? t.usedToday.replace('{used}', compact(budget.used))
                : t.usedOfLimit.replace('{used}', compact(budget.used)).replace('{limit}', compact(budget.limit))
              : week
                ? t.efiLeverWeekFlat.replace('{tokens}', compact(week.this))
                : t.noLimit}
          </p>
          {budget && budget.limit !== null && (
            <div className="efi-micro-barra" aria-hidden="true">
              <i className={budgetFill >= 0.85 ? 'quente' : budgetFill >= 0.6 ? 'alerta' : 'ok'} style={{ width: `${Math.max(3, budgetFill * 100)}%` }} />
            </div>
          )}
          {budget && (
            <div className="efi-alavanca-chips">
              <button
                type="button"
                className={`efi-chip-mini ${budget.limit === null ? 'ativo' : ''}`}
                disabled={busyKey === 'limit'}
                onClick={() => void setDailyLimit(0)}
              >
                {t.noLimit}
              </button>
              {budget.choices.slice(0, 4).map((choice) => (
                <button
                  key={choice}
                  type="button"
                  className={`efi-chip-mini ${budget.limit === choice ? 'ativo' : ''}`}
                  disabled={busyKey === 'limit'}
                  onClick={() => void setDailyLimit(choice)}
                >
                  {compact(choice)}
                </button>
              ))}
            </div>
          )}
        </article>

        {/* 3. Dev/Infra Auditor */}
        <article className="efi-alavanca">
          <div className="efi-alavanca-topo">
            <h3>{t.efiLeverAuditorTitle}</h3>
            {auditor?.enabled && <span className="efi-tag ok">{t.turnedOn}</span>}
          </div>
          <p className="efi-alavanca-texto">
            {auditor?.enabled
              ? t.efiLeverAuditorOn
                  .replace('{cadence}', t.auditorCadences[auditor.cadence] ?? auditor.cadence)
                  .replace('{last}', auditorLast)
              : t.efiLeverAuditorOff}
          </p>
          <div className="efi-alavanca-acoes">
            {auditor?.enabled ? (
              <button type="button" className="efi-pill-cta primario" disabled={auditing || targetBot.busy} onClick={onRunAudit}>
                {auditing ? t.auditorRunning : t.efiLeverAuditorRun}
              </button>
            ) : canEnableAuditor ? (
              <button type="button" className="efi-pill-cta primario" disabled={auditing || targetBot.busy} onClick={onRunAudit}>
                {auditing ? t.auditorRunning : t.efiLeverAuditorEnable}
              </button>
            ) : null}
            {auditorThread && (
              <button type="button" className="efi-pill-cta suave" onClick={() => onOpenAuditThread(auditorThread)}>
                {t.efiLeverAuditorSee}
              </button>
            )}
          </div>
        </article>

        {/* 4. Fixed Wiki & Weekly Peak */}
        <article className="efi-alavanca">
          <div className="efi-alavanca-topo">
            <h3>{t.efiLeverWikiTitle}</h3>
            {alwaysPages.length > 0 && <span className="efi-tag">{`~${compact(alwaysTokens)}`}</span>}
          </div>
          <p className="efi-alavanca-texto">
            {alwaysPages.length > 0
              ? t.efiLeverWikiAlways.replace('{n}', String(alwaysPages.length)).replace('{tokens}', compact(alwaysTokens))
              : t.efiLeverWikiNone}
            {week?.top ? ` · ${t.efiLeverWeekTop.replace('{tool}', week.top.tool).replace('{tokens}', compact(week.top.tokens))}` : ''}
          </p>
          <div className="efi-alavanca-acoes">
            <button type="button" className="efi-pill-cta" onClick={onOpenWiki}>
              {t.efiKpiStartupCtaWiki}
            </button>
            <button type="button" className="efi-pill-cta suave" onClick={onOpenBoard}>
              {t.efiCtaSeeBoard}
            </button>
          </div>
        </article>
      </div>
    </section>
  )
}
