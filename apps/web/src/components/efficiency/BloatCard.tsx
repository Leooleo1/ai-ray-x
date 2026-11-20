import { IS_REMOTE, type Bot, type Carried, type Change } from '../../api'
import { compact, percent, t } from '../../i18n'

const DISPLAY: Record<string, string> = { 'your prompts': t.yourPrompts, 'injected text': t.injected }

interface BloatCardProps {
  carried: Carried[]
  costShares: [string, number][]
  applicable: string[]
  openChanges: Change[]
  targetBot: Bot | undefined
  auditing: boolean
  shotsTaskCreated: boolean
  onFocusTrim: () => void
  onCreateShotsTask: () => void
  onRunAudit: () => void
  onOpenWiki: () => void
  onOpenBoard: () => void
}

/** Bento 3: Weighted cost breakdown + top items carried in context, each paired with a contextual CTA. */
export function BloatCard({
  carried,
  costShares,
  applicable,
  openChanges,
  targetBot,
  auditing,
  shotsTaskCreated,
  onFocusTrim,
  onCreateShotsTask,
  onRunAudit,
  onOpenWiki,
  onOpenBoard,
}: BloatCardProps) {
  const shareOf = (prefix: string) => costShares.filter(([k]) => k.startsWith(prefix)).reduce((sum, [, v]) => sum + v, 0)
  const segments = [
    { key: 'cache_read', label: t.efiCostCacheRead, share: shareOf('cache_read') },
    { key: 'cache_write', label: t.efiCostCacheWrite, share: shareOf('cache_write') },
    { key: 'input', label: t.efiCostInput, share: shareOf('input') },
    { key: 'output', label: t.efiCostOutput, share: shareOf('output') },
  ].filter((s) => s.share > 0.004)

  const top = carried.slice(0, 6)
  const maxCarried = Math.max(1, ...top.map((c) => c.carried_tokens))
  const trimApplied = openChanges.some((c) => c.suggestion === 'trim-tool-output')
  const canTrim = applicable.includes('trim-tool-output') && !trimApplied
  const canAudit = Boolean(targetBot && (targetBot.auditor?.enabled || !IS_REMOTE))

  return (
    <section className="cartao efi-peso">
      <header className="efi-card-topo">
        <h2>
          {t.efiBloatTitle}
          <small>{t.efiBloatHint}</small>
        </h2>
      </header>

      {segments.length > 0 && (
        <>
          <div className="efi-custo-barra" role="img" aria-label={t.efiBloatHint}>
            {segments.map((seg) => (
              <div
                key={seg.key}
                className={`efi-custo-seg ${seg.key}`}
                style={{ width: `${seg.share * 100}%` }}
                title={`${seg.label}: ${percent(seg.share)}`}
              />
            ))}
          </div>
          <div className="efi-custo-legenda">
            {segments.map((seg) => (
              <span key={seg.key}>
                <i className={`efi-custo-ponto ${seg.key}`} aria-hidden="true" />
                {`${seg.label} ${percent(seg.share)}`}
              </span>
            ))}
          </div>
        </>
      )}

      <ul className="efi-peso-lista">
        {top.map((item) => {
          const isBash = item.name === 'Bash' || item.name === 'exec'
          const hasImages = item.images > 0
          const isInjected = item.name === 'injected text'
          const fill = Math.max(3, (item.carried_tokens / maxCarried) * 100)

          return (
            <li key={item.name} className="efi-peso-item">
              <div className="efi-peso-linha">
                <div className="efi-peso-esq">
                  <span className="efi-peso-nome">{DISPLAY[item.name] ?? item.name}</span>
                  <span className="efi-peso-meta">{t.efiCarriedResults.replace('{n}', compact(item.results))}</span>
                  {hasImages && <span className="efi-tag alerta">{t.efiCarriedImages.replace('{n}', compact(item.images))}</span>}
                </div>

                <div className="efi-peso-dir">
                  <span className="efi-peso-num">{compact(item.carried_tokens)}</span>
                  <span className="efi-peso-pct">{percent(item.share_of_context)}</span>

                  {isBash && canTrim && (
                    <button type="button" className="efi-chip-mini" onClick={onFocusTrim}>
                      {t.efiBloatCtaTrim}
                    </button>
                  )}

                  {!isBash && hasImages && targetBot && (
                    <button
                      type="button"
                      className={`efi-chip-mini ${shotsTaskCreated ? 'ativo' : ''}`}
                      onClick={shotsTaskCreated ? onOpenBoard : onCreateShotsTask}
                    >
                      {shotsTaskCreated ? t.efiCtaSeeBoard : t.efiBloatCtaShots}
                    </button>
                  )}

                  {isInjected && (
                    <button type="button" className="efi-chip-mini" disabled={auditing} onClick={canAudit ? onRunAudit : onOpenWiki}>
                      {canAudit ? t.efiBloatCtaAudit : t.efiCtaOpenWiki}
                    </button>
                  )}
                </div>
              </div>
              <div className="efi-micro-barra" aria-hidden="true">
                <i className={item.share_of_context >= 0.25 ? 'quente' : ''} style={{ width: `${fill}%` }} />
              </div>
            </li>
          )
        })}
      </ul>
    </section>
  )
}
