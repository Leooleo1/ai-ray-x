import { useEffect, useState } from 'react'
import { IS_REMOTE, type Bot, type Change, type Outcome, type Source, type SourceReport } from '../../api'
import { dietLines } from '../../diet'
import { compact, locale, percent, t } from '../../i18n'
import { Action } from '../Action'

const QUALITY_LIMIT = 1.0
const per100 = (n: number): string => n.toLocaleString(locale, { maximumFractionDigits: 1 })

interface CutsCardProps {
  selected: SourceReport
  source: Source
  targetDir: string
  targetName: string
  targetBot: Bot | undefined
  applicable: string[]
  openChanges: Change[]
  outcomes: Outcome[]
  highlighted: boolean
  auditing: boolean
  createdTasks: Record<string, boolean>
  onChanged: () => void
  onRunAudit: () => void
  onNewCleanChat: () => void
  onOpenWiki: () => void
  onOpenBoard: () => void
  onOpenChats: () => void
  onCreateTask: (kind: 'shots' | 'startup') => void
}

/** Bento 1: Interactive compaction threshold simulator + prioritized action queue. One filled action, quiet text for the rest. */
export function CutsCard({
  selected,
  source,
  targetDir,
  targetName,
  targetBot,
  applicable,
  openChanges,
  outcomes,
  highlighted,
  auditing,
  createdTasks,
  onChanged,
  onRunAudit,
  onNewCleanChat,
  onOpenWiki,
  onOpenBoard,
  onOpenChats,
  onCreateTask,
}: CutsCardProps) {
  const model = selected.efficiency.compaction
  const lines = dietLines(selected)
  const compactChange = openChanges.find((c) => c.suggestion === 'compact-earlier')
  const trimChange = openChanges.find((c) => c.suggestion === 'trim-tool-output')
  const totalContext = Math.max(1, selected.efficiency.context.total)

  const defaultCap = (() => {
    if (compactChange) {
      const appliedCap = Number(compactChange.after)
      if (model.plans.some((p) => p.cap === appliedCap)) return appliedCap
    }
    return model.best?.cap ?? model.plans[2]?.cap ?? model.plans[0]?.cap ?? 400_000
  })()

  const [selectedCap, setSelectedCap] = useState<number>(defaultCap)
  useEffect(() => {
    setSelectedCap(defaultCap)
  }, [defaultCap, source, targetDir])

  const trimChoices = selected.efficiency.output_trim?.length
    ? selected.efficiency.output_trim
    : source === 'claude'
      ? ([['Bash', 15_000, 0], ['Bash', 10_000, 0]] as [string, number, number][])
      : []

  const defaultTrim = trimChange ? Number(trimChange.after) || 15_000 : 15_000
  const [selectedTrim, setSelectedTrim] = useState<number>(defaultTrim)
  useEffect(() => {
    setSelectedTrim(defaultTrim)
  }, [defaultTrim, source, targetDir])

  const activePlan = model.plans.find((p) => p.cap === selectedCap) ?? model.best ?? model.plans[0] ?? null
  const planSavingTokens = activePlan && activePlan.net_saving > 0 ? Math.round(activePlan.net_saving * totalContext) : 0
  const overQualityGuard = Boolean(activePlan && activePlan.compactions_per_100 > QUALITY_LIMIT + 1e-6)
  const canApplyCompact = applicable.includes('compact-earlier')
  const canApplyTrim = applicable.includes('trim-tool-output')
  const canAudit = Boolean(targetBot && (targetBot.auditor?.enabled || !IS_REMOTE))

  const otherLines = lines.filter((line) => line.id !== 'compact-earlier')
  const trimInQueue = otherLines.some((l) => l.id === 'trim-tool-output')
  const bashTrim15 = trimChoices.find(([, limit]) => limit === 15_000)?.[2] ?? 0
  const bashCarried = selected.efficiency.carried.find((c) => c.name === 'Bash')?.carried_tokens ?? 0
  const queueLines =
    !trimInQueue && canApplyTrim && (bashTrim15 > 0 || bashCarried > 0)
      ? [
          ...otherLines,
          {
            id: 'trim-tool-output',
            title: locale === 'pt-BR' ? 'Encurtar a saída das ferramentas' : 'Trim tool output',
            evidence:
              bashTrim15 > 0
                ? locale === 'pt-BR'
                  ? `limitar a saída do Bash em 15 mil caracteres corta ${compact(bashTrim15)} tokens relidos`
                  : `capping Bash output at 15K characters cuts ${compact(bashTrim15)} re-read tokens`
                : locale === 'pt-BR'
                  ? `a saída do Bash fica carregada: ${compact(bashCarried)} tokens relidos`
                  : `Bash output stays carried: ${compact(bashCarried)} re-read tokens`,
            how:
              locale === 'pt-BR'
                ? 'Grava BASH_MAX_OUTPUT_LENGTH no .claude/settings.local.json deste projeto'
                : 'Writes BASH_MAX_OUTPUT_LENGTH in this project’s .claude/settings.local.json',
            saving: bashTrim15,
            share: bashTrim15 / totalContext,
            value: '15000',
          },
        ]
      : otherLines

  return (
    <section id="efi-simulador" className={`cartao efi-simulador ${highlighted ? 'efi-foco' : ''}`}>
      <header className="efi-card-topo">
        <h2>
          {t.efiSimTitle}
          <small>{t.efiSimHint}</small>
        </h2>
        {compactChange && <span className="efi-meta ok">{`${t.applied} · ${compact(Number(compactChange.after))}`}</span>}
      </header>

      {activePlan ? (
        <div className="efi-sim">
          <div className="efi-sim-tabs" role="radiogroup" aria-label={t.efiSimTitle}>
            {model.plans.map((plan) => {
              const isSelected = plan.cap === activePlan.cap
              const isBest = model.best?.cap === plan.cap
              const isApplied = compactChange && Number(compactChange.after) === plan.cap
              return (
                <button
                  key={plan.cap}
                  type="button"
                  role="radio"
                  aria-checked={isSelected}
                  className={`efi-sim-tab ${isSelected ? 'ativo' : ''}`}
                  onClick={() => setSelectedCap(plan.cap)}
                >
                  {isApplied && <i className="ponto ok" aria-hidden="true" />}
                  <span>{compact(plan.cap)}</span>
                  {isBest && <span className="efi-sim-ideal">{t.efiSimRecommended}</span>}
                </button>
              )
            })}
          </div>

          <div className="efi-sim-metricas">
            <div className="efi-sim-cel">
              <span>{t.rereadPerRequest}</span>
              <strong>{compact(activePlan.avg_context)}</strong>
              <em>{`${t.today} ${compact(model.now_avg_context)}`}</em>
            </div>
            <div className="efi-sim-cel">
              <span>{t.summariesPer100}</span>
              <strong className={overQualityGuard ? 'alerta' : ''}>{per100(activePlan.compactions_per_100)}</strong>
              <em>{`${t.today} ${per100(model.now_compactions_per_100)}`}</em>
            </div>
            <div className="efi-sim-cel">
              <span>{t.efiSimNetSaving}</span>
              <strong className={activePlan.net_saving > 0 ? 'ok' : ''}>
                {activePlan.net_saving > 0 ? `~${percent(activePlan.net_saving)}` : '0%'}
              </strong>
              <em>{planSavingTokens > 0 ? `-${compact(planSavingTokens)}` : '—'}</em>
            </div>
            <div className="efi-sim-cel">
              <span>{t.afterSummary}</span>
              <strong>{compact(model.floor + model.refill)}</strong>
              <em>{`${compact(model.floor)} + ${compact(model.refill)}`}</em>
            </div>
          </div>

          {overQualityGuard && <div className="efi-aviso">{t.efiSimQualityWarn}</div>}

          {canApplyCompact && (
            <div className="efi-acoes">
              <Action
                key={`compact-earlier:${targetDir}:${activePlan.cap}`}
                id="compact-earlier"
                value={String(activePlan.cap)}
                source={source}
                change={compactChange}
                outcome={outcomes.find((o) => o.change_id === compactChange?.id)}
                onChanged={onChanged}
                bot={targetBot?.id}
                project={targetName}
                label={t.efiSimApplyCap.replace('{cap}', compact(activePlan.cap))}
                primary
              />
            </div>
          )}
        </div>
      ) : (
        <p className="efi-vazio">{t.efiSimNoPlans}</p>
      )}

      <div className="efi-divisor" />
      <h3 className="efi-fila-titulo">{t.efiCutsHeading}</h3>
      {queueLines.length === 0 ? (
        <p className="efi-vazio">{t.efiCutsEmpty}</p>
      ) : (
        <ol className="efi-fila" id="efi-cortes">
          {queueLines.map((line) => {
            const isTrim = line.id === 'trim-tool-output'
            const isStartup = line.id === 'lighter-startup'
            const isBreaks = line.id === 'fresh-after-breaks'
            const isShots = line.id === 'screenshots-in-subagents'
            const chosenTrimEntry = trimChoices.find(([, limit]) => limit === selectedTrim) ?? trimChoices[0]
            const activeTrimSaving = chosenTrimEntry ? chosenTrimEntry[2] : line.saving
            const activeTrimShare = chosenTrimEntry ? chosenTrimEntry[2] / totalContext : line.share

            return (
              <li key={line.id} className="efi-corte" id={`efi-corte-${line.id}`}>
                <div className="efi-corte-topo">
                  <span className="efi-corte-nome">{line.title}</span>
                  {(isTrim ? activeTrimSaving : line.saving) > 0 && (
                    <span className="efi-corte-ganho">
                      {`-${compact(isTrim ? activeTrimSaving : line.saving)}`}
                      {(isTrim ? activeTrimShare : line.share) > 0 && (
                        <small>{`~${percent(isTrim ? activeTrimShare : line.share)}`}</small>
                      )}
                    </span>
                  )}
                </div>
                <div className="efi-corte-texto">{line.evidence}</div>

                <div className="efi-acoes">
                  {isTrim && canApplyTrim && (
                    <>
                      {trimChoices.length > 1 &&
                        trimChoices.map(([, limit, saved]) => (
                          <button
                            key={limit}
                            type="button"
                            className={`acao-texto ${selectedTrim === limit ? 'ativo' : ''}`}
                            onClick={() => setSelectedTrim(limit)}
                            title={saved > 0 ? t.efiLimitSaves.replace('{tokens}', compact(saved)) : undefined}
                          >
                            {`${compact(limit)} ${t.efiCharsShort}`}
                          </button>
                        ))}
                      <Action
                        key={`trim-tool-output:${targetDir}:${selectedTrim}`}
                        id="trim-tool-output"
                        value={String(selectedTrim)}
                        source={source}
                        change={trimChange}
                        outcome={outcomes.find((o) => o.change_id === trimChange?.id)}
                        onChanged={onChanged}
                        bot={targetBot?.id}
                        project={targetName}
                        label={t.efiApplyTrim.replace('{limit}', compact(selectedTrim))}
                        quiet
                      />
                    </>
                  )}

                  {isStartup && (
                    <>
                      {canAudit && (
                        <button type="button" className="acao-texto" disabled={auditing} onClick={onRunAudit}>
                          {auditing
                            ? t.auditorRunning
                            : targetBot?.auditor?.enabled
                              ? t.efiCtaAuditInfra
                              : t.efiCtaAuditEnableAndRun}
                        </button>
                      )}
                      <button type="button" className="acao-texto" onClick={onOpenWiki}>
                        {t.efiCtaOpenWiki}
                      </button>
                      {targetBot &&
                        (createdTasks.startup ? (
                          <button type="button" className="acao-texto ativo" onClick={onOpenBoard}>
                            {`${t.efiCtaTaskCreated} · ${t.efiCtaSeeBoard}`}
                          </button>
                        ) : (
                          <button type="button" className="acao-texto" onClick={() => onCreateTask('startup')}>
                            {t.efiCtaCreateTask}
                          </button>
                        ))}
                    </>
                  )}

                  {isBreaks &&
                    (targetBot ? (
                      <button type="button" className="acao-texto" onClick={onNewCleanChat}>
                        {t.efiCtaNewCleanChat}
                      </button>
                    ) : (
                      <button type="button" className="acao-texto" onClick={onOpenChats}>
                        {t.efiCtaOpenChat}
                      </button>
                    ))}

                  {isShots &&
                    (targetBot ? (
                      createdTasks.shots ? (
                        <button type="button" className="acao-texto ativo" onClick={onOpenBoard}>
                          {`${t.efiCtaTaskCreated} · ${t.efiCtaSeeBoard}`}
                        </button>
                      ) : (
                        <button type="button" className="acao-texto" onClick={() => onCreateTask('shots')}>
                          {t.efiCtaCreateTask}
                        </button>
                      )
                    ) : (
                      <button type="button" className="acao-texto" onClick={onOpenChats}>
                        {t.efiCtaOpenChat}
                      </button>
                    ))}
                </div>
              </li>
            )
          })}
        </ol>
      )}
    </section>
  )
}
