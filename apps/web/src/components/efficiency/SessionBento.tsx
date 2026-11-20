import type { Bot, ContextStatus, Efficiency } from '../../api'
import { compact, percent, t } from '../../i18n'
import { PixelHistogram } from './PixelField'

const RULER_SCALE = 1_000_000
const NEAR_START = 0.08
const DISPLAY: Record<string, string> = { 'your prompts': t.yourPrompts, 'injected text': t.injected }

const BUCKET_LABELS = ['<50K', '100K', '200K', '400K', '600K', '800K', '800K+']

interface SessionBentoProps {
  status: ContextStatus | null | undefined
  targetName: string
  targetBot: Bot | undefined
  buckets: Efficiency['buckets']
  onNewCleanChat: () => void
  onOpenChat: () => void
}

/** Bento 2: Raio's live session ruler + 7-column pixel histogram + clean-chat CTAs. */
export function SessionBento({ status, targetName, targetBot, buckets, onNewCleanChat, onOpenChat }: SessionBentoProps) {
  const at = (value: number) => ({ left: `${Math.min(100, (value / RULER_SCALE) * 100)}%` })
  const maxBucketShare = Math.max(0.01, ...buckets.map((b) => b.share_of_tokens))
  const heavyTokens = buckets.filter((b) => b.up_to === null || b.up_to > 400_000).reduce((sum, b) => sum + b.share_of_tokens, 0)
  const isHeavyLive = Boolean(
    status && ((status.usual_compaction_at > 0 && status.context_tokens / status.usual_compaction_at >= 0.55) || status.context_tokens >= 300_000),
  )

  const columns = buckets.slice(0, 7).map((bucket, idx) => {
    const label = BUCKET_LABELS[idx] ?? (bucket.up_to ? compact(bucket.up_to) : '800K+')
    const heavy = bucket.up_to === null || bucket.up_to > 400_000
    return {
      label,
      ratio: bucket.share_of_tokens / maxBucketShare,
      valueLabel: bucket.share_of_tokens >= 0.01 ? percent(bucket.share_of_tokens) : '',
      title: `${label}: ${t.efiBucketsCallsLabel.replace('{calls}', percent(bucket.share_of_calls)).replace('{tokens}', percent(bucket.share_of_tokens))}`,
      color: heavy ? 'var(--brasa)' : 'var(--cobalto)',
    }
  })

  return (
    <section className="cartao efi-sessao">
      <div className="efi-sessao-bloco">
        <header className="efi-card-topo">
          <h2>
            {t.sessionTitle}
            {status && <small>{`${status.project.split('/').pop()} · ${status.requests_since_compaction} ${t.requests}`}</small>}
          </h2>
        </header>

        {status ? (
          <>
            <div className={`trilho-wrap ${status.source}`}>
              <div className="trilho" />
              {status.usual_compaction_at > 0 && <span className="corte" style={at(status.usual_compaction_at)} />}
              <span className="marcador mediana" style={at(status.context_tokens)} />
              <span
                className={`rotulo-marca acima ${status.context_tokens / RULER_SCALE < NEAR_START ? 'do-inicio' : ''}`}
                style={at(status.context_tokens)}
              >
                {t.now} {compact(status.context_tokens)}
              </span>
              {status.usual_compaction_at > 0 && (
                <span className="rotulo-marca abaixo antes" style={at(status.usual_compaction_at)}>
                  {t.compacts} {compact(status.usual_compaction_at)}
                </span>
              )}
              <span className="rotulo-escala inicio">0</span>
            </div>

            <ul className="efi-sessao-lista">
              <li className="efi-sessao-item">
                <span className="efi-sessao-item-nome">{t.startup}</span>
                <span className="efi-sessao-item-num">{compact(status.startup_tokens)}</span>
                <span className="efi-sessao-item-pct">{percent(status.startup_tokens / Math.max(1, status.context_tokens))}</span>
              </li>
              {status.loaded_since_compaction.slice(0, 5).map((item) => (
                <li key={item.name} className="efi-sessao-item">
                  <span className="efi-sessao-item-nome">{DISPLAY[item.name] ?? item.name}</span>
                  <span className="efi-sessao-item-num">{compact(item.tokens)}</span>
                  <span className="efi-sessao-item-pct">{percent(item.tokens / Math.max(1, status.context_tokens))}</span>
                </li>
              ))}
            </ul>
          </>
        ) : (
          <p className="efi-vazio">{status === undefined ? t.probing : t.noSessionIn.replace('{p}', targetName)}</p>
        )}
      </div>

      {columns.length > 0 && (
        <>
          <div className="efi-divisor" />
          <div className="efi-sessao-bloco">
            <div className="efi-faixas-topo">
              <h3>{t.efiBucketsTitle}</h3>
              <span className="efi-meta">{t.efiBucketsHeavyNote.replace('{share}', percent(heavyTokens))}</span>
            </div>
            <PixelHistogram columns={columns} rows={9} subCols={4} label={t.efiBucketsTitle} />
          </div>
        </>
      )}

      {targetBot && (
        <div className="efi-acoes">
          <button type="button" className={isHeavyLive ? 'projeto-acao cheio' : 'acao-texto'} onClick={onNewCleanChat}>
            {t.efiCtaNewCleanChat}
          </button>
          <button type="button" className="acao-texto" onClick={onOpenChat}>
            {t.efiCtaOpenChat}
          </button>
        </div>
      )}
    </section>
  )
}
