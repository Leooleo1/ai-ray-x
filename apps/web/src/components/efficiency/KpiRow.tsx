import { AGENTS } from '../../agents'
import { IS_REMOTE, type Bot, type Change, type ContextStatus, type Outcome, type SourceReport } from '../../api'
import { compact, percent, splitCompact, t, usd } from '../../i18n'
import { Avatar } from '../Avatar'

interface KpiRowProps {
  selected: SourceReport
  status: ContextStatus | null | undefined
  targetName: string
  targetBot: Bot | undefined
  openChanges: Change[]
  outcomes: Outcome[]
  lessons: number
  auditing: boolean
  onJumpSimulator: () => void
  onJumpCuts: () => void
  onJumpLedger: () => void
  onRunAudit: () => void
  onNewCleanChat: () => void
  onOpenChat: () => void
  onOpenWiki: () => void
}

/** Four actionable KPI cards at the top of the Efficiency tab: every metric carries a direct CTA. */
export function KpiRow({
  selected,
  status,
  targetName,
  targetBot,
  openChanges,
  outcomes,
  lessons,
  auditing,
  onJumpSimulator,
  onJumpCuts,
  onJumpLedger,
  onRunAudit,
  onNewCleanChat,
  onOpenChat,
  onOpenWiki,
}: KpiRowProps) {
  const eff = selected.efficiency
  const topSuggestion = selected.suggestions[0]
  const bestShare = topSuggestion?.saving_share ?? 0
  const bestTokens = topSuggestion?.saving_tokens ?? 0
  const estUsd = selected.overview.reported_cost_usd && bestShare > 0 ? selected.overview.reported_cost_usd * bestShare : null
  const topAlreadyApplied = Boolean(topSuggestion && openChanges.some((c) => c.suggestion === topSuggestion.id))

  const heavyCalls = eff.buckets.filter((b) => b.up_to === null || b.up_to > 400_000).reduce((sum, b) => sum + b.share_of_calls, 0)
  const cacheReadShare = eff.cost_shares.find(([kind]) => kind === 'cache_read')?.[1] ?? 0

  const mainFixed = eff.fixed.find((f) => f.kind === 'main')
  const startupTokens = mainFixed?.median_first_request ?? 0
  const startupShare = mainFixed?.share_of_context ?? 0
  const listedSkills = Number(eff.snapshots.skill_listing_items ?? 0)
  const usedSkills = eff.skills_used.length

  const liveFill = status && status.usual_compaction_at > 0 ? Math.min(1, status.context_tokens / status.usual_compaction_at) : 0
  const isHeavyLive = Boolean(status && (liveFill >= 0.55 || status.context_tokens >= 300_000))
  const measured = openChanges
    .map((c) => outcomes.find((o) => o.change_id === c.id && o.conclusive))
    .find((o): o is Outcome => Boolean(o))

  const savingBig = percent(bestShare)
  const startupSplit = splitCompact(startupTokens)
  const sessionSplit = status ? splitCompact(status.context_tokens) : null
  const canAudit = Boolean(targetBot && (targetBot.auditor?.enabled || !IS_REMOTE))

  return (
    <>
      <section className="cartao efi-kpi efi-kpi-heroi">
        <div className="efi-kpi-topo">
          <span className="efi-kpi-rotulo">
            <Avatar agent={AGENTS.dieta} size={24} followPointer state="idle" />
            {t.efiKpiSavingTitle}
          </span>
          {estUsd !== null && estUsd >= 1 && <span className="efi-tag brasa">{t.efiKpiSavingUsd.replace('{usd}', usd(estUsd))}</span>}
        </div>
        <div className="efi-kpi-corpo">
          <div className="efi-kpi-valor">{bestShare > 0 ? `~${savingBig}` : '0%'}</div>
          <span className="efi-kpi-sub">
            {bestTokens > 0 ? t.efiKpiSavingSub.replace('{tokens}', compact(bestTokens)) : t.efiKpiSavingLean}
          </span>
        </div>
        <div className="efi-kpi-rodape">
          {bestShare > 0 && !topAlreadyApplied ? (
            <button type="button" className="efi-pill-cta primario" onClick={onJumpCuts}>
              {t.efiKpiSavingCta}
            </button>
          ) : openChanges.length > 0 ? (
            <button type="button" className="efi-pill-cta ok" onClick={onJumpLedger}>
              {t.efiKpiSavingCtaDone}
            </button>
          ) : (
            <button type="button" className="efi-pill-cta" onClick={canAudit ? onRunAudit : onOpenWiki} disabled={auditing}>
              {auditing ? t.auditorRunning : canAudit ? t.efiKpiStartupCtaAudit : t.efiKpiStartupCtaWiki}
            </button>
          )}
        </div>
      </section>

      <section className="cartao efi-kpi">
        <div className="efi-kpi-topo">
          <span className="efi-kpi-rotulo">{t.efiKpiContextTitle}</span>
          {heavyCalls >= 0.1 && <span className="efi-tag alerta">{`${percent(heavyCalls)} >400K`}</span>}
        </div>
        <div className="efi-kpi-corpo">
          <div className="efi-kpi-valor">
            <span>{compact(eff.context.median)}</span>
            <span className="efi-kpi-seta">→</span>
            <span>{compact(eff.context.p90)}</span>
          </div>
          <span className="efi-kpi-sub">
            {t.efiKpiContextSub.replace('{heavy}', percent(heavyCalls)).replace('{reread}', percent(cacheReadShare))}
          </span>
        </div>
        <div className="efi-kpi-rodape">
          <button type="button" className="efi-pill-cta" onClick={onJumpSimulator}>
            {t.efiKpiContextCta}
          </button>
        </div>
      </section>

      <section className="cartao efi-kpi">
        <div className="efi-kpi-topo">
          <span className="efi-kpi-rotulo">{t.efiKpiStartupTitle}</span>
          {listedSkills > 0 && usedSkills < listedSkills && (
            <span className="efi-tag">{`${usedSkills}/${listedSkills} skills`}</span>
          )}
        </div>
        <div className="efi-kpi-corpo">
          <div className="efi-kpi-valor">
            <span>{startupSplit.value}</span>
            {startupSplit.unit && <span className="unidade-pequena">{startupSplit.unit}</span>}
          </div>
          <span className="efi-kpi-sub">
            {listedSkills > 0
              ? t.efiKpiStartupSub
                  .replace('{share}', percent(startupShare))
                  .replace('{used}', String(usedSkills))
                  .replace('{listed}', String(listedSkills))
              : t.efiKpiStartupSubNoSkills.replace('{share}', percent(startupShare))}
          </span>
        </div>
        <div className="efi-kpi-rodape">
          {canAudit ? (
            <button type="button" className="efi-pill-cta" disabled={auditing} onClick={onRunAudit}>
              {auditing ? t.auditorRunning : t.efiKpiStartupCtaAudit}
            </button>
          ) : (
            <button type="button" className="efi-pill-cta" onClick={onOpenWiki}>
              {t.efiKpiStartupCtaWiki}
            </button>
          )}
        </div>
      </section>

      <section className="cartao efi-kpi">
        <div className="efi-kpi-topo">
          <span className="efi-kpi-rotulo">
            <Avatar agent={AGENTS.raio} size={22} state="still" />
            {t.efiKpiSessionTitle}
          </span>
          {measured && (
            <span className={`efi-tag ${measured.context_change <= 0 ? 'ok' : 'alerta'}`}>
              {`${measured.context_change > 0 ? '+' : ''}${percent(measured.context_change)}`}
            </span>
          )}
        </div>
        <div className="efi-kpi-corpo">
          <div className="efi-kpi-valor">
            {status === undefined ? (
              '…'
            ) : sessionSplit ? (
              <>
                <span>{sessionSplit.value}</span>
                {sessionSplit.unit && <span className="unidade-pequena">{sessionSplit.unit}</span>}
              </>
            ) : (
              t.efiKpiSessionEmpty
            )}
          </div>
          {status && status.usual_compaction_at > 0 && (
            <div className="efi-micro-barra" aria-hidden="true">
              <i className={isHeavyLive ? 'quente' : ''} style={{ width: `${Math.max(4, liveFill * 100)}%` }} />
            </div>
          )}
          <span className="efi-kpi-sub">
            {status
              ? `${targetName} · ${t.efiKpiSessionSub.replace('{active}', String(openChanges.length)).replace('{lessons}', String(lessons))}`
              : t.efiKpiSessionSub.replace('{active}', String(openChanges.length)).replace('{lessons}', String(lessons))}
          </span>
        </div>
        <div className="efi-kpi-rodape">
          {targetBot ? (
            isHeavyLive ? (
              <button type="button" className="efi-pill-cta primario" onClick={onNewCleanChat}>
                {t.efiKpiSessionCtaClean}
              </button>
            ) : (
              <button type="button" className="efi-pill-cta" onClick={onOpenChat}>
                {t.efiKpiSessionCtaOpen}
              </button>
            )
          ) : (
            <button type="button" className="efi-pill-cta" onClick={onJumpCuts}>
              {t.efiKpiSessionCtaCuts}
            </button>
          )}
        </div>
      </section>
    </>
  )
}
