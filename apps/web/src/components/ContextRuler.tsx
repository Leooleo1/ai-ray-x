import type { SourceReport } from '../api'
import { compact, fill, percent, t } from '../i18n'
import { heavyShare, rereadShare } from '../model'

const SCALE = 1_000_000
/** Markers closer than this share of the ruler share one label, or their labels would overlap. */
const CLOSE = 0.25
/** Past this point a shared label grows to the left, to stay inside the ruler. */
const LATE = 0.6

interface ContextRulerProps {
  sources: SourceReport[]
  main: SourceReport
  /** Opens Efficiency, where the cuts are. */
  onCut: () => void
}

/** Dotted ruler from 0 to 1M tokens, with the median, the 90th percentile and the compaction point. */
export function ContextRuler({ sources, main, onCut }: ContextRulerProps) {
  const summary = `${fill(t.overShare, percent(heavyShare(main)))} · ${fill(t.rereadShare, percent(rereadShare(main)))}`
  return (
    <section className="cartao regua">
      <h2>
        {t.context}
        <small>{summary}</small>
        <button type="button" className="lado-link regua-acao" onClick={onCut}>
          {t.howToCut}
        </button>
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
        {(p90 - median) / SCALE < CLOSE ? (
          <span className={`rotulo-marca acima ${median / SCALE > LATE ? 'junto-fim' : 'junto'}`} style={at(median / SCALE > LATE ? p90 : median)}>
            {`${t.median} ${compact(median)} · ${t.p90} ${compact(p90)}`}
          </span>
        ) : (
          <>
            <span className="rotulo-marca acima" style={at(median)}>
              {t.median} {compact(median)}
            </span>
            <span className="rotulo-marca acima" style={at(p90)}>
              {t.p90} {compact(p90)}
            </span>
          </>
        )}
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
