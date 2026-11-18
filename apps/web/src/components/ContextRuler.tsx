import type { SourceReport } from '../api'
import { compact, fill, percent, t } from '../i18n'
import { heavyShare, rereadShare } from '../model'

const SCALE = 1_000_000

interface ContextRulerProps {
  sources: SourceReport[]
  main: SourceReport
}

/** Dotted ruler from 0 to 1M tokens, with the median, the 90th percentile and the compaction point. */
export function ContextRuler({ sources, main }: ContextRulerProps) {
  const summary = `${fill(t.overShare, percent(heavyShare(main)))} · ${fill(t.rereadShare, percent(rereadShare(main)))}`
  return (
    <section className="cartao regua">
      <h2>
        {t.context}
        <small>{summary}</small>
      </h2>
      <div className="linhas">
        {sources.map((source) => (
          <RulerRow key={source.overview.source} source={source} />
        ))}
      </div>
    </section>
  )
}

interface RulerRowProps {
  source: SourceReport
}

function RulerRow({ source }: RulerRowProps) {
  const { median, p90 } = source.efficiency.context
  const compaction = source.efficiency.compaction_pre_median
  const at = (value: number) => ({ left: `${Math.min(100, (value / SCALE) * 100)}%` })
  const name = source.overview.source === 'claude' ? 'Claude Code' : 'Codex'
  return (
    <div className={`linha-regua ${source.overview.source}`}>
      <span className="fonte">
        <i className={`ponto ${source.overview.source}`} />
        {name}
      </span>
      <div className="trilho-wrap">
        <div className="trilho" />
        {compaction > 0 && <span className="corte" style={at(compaction)} />}
        <span className="marcador mediana" style={at(median)} />
        <span className="marcador p90" style={at(p90)} />
        <span className="rotulo-marca acima" style={at(median)}>
          {t.median} {compact(median)}
        </span>
        <span className="rotulo-marca acima" style={at(p90)}>
          {t.p90} {compact(p90)}
        </span>
        {compaction > 0 && (
          <span className="rotulo-marca abaixo antes" style={at(compaction)}>
            {t.compacts} {compact(compaction)}
          </span>
        )}
        <span className="rotulo-escala inicio">0</span>
        <span className="rotulo-escala fim">{compact(SCALE)}</span>
      </div>
    </div>
  )
}
