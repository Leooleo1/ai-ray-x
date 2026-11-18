import { useState } from 'react'
import { AGENTS, type Agent } from '../agents'
import type { ContextStatus, Report, Source } from '../api'
import { dietLines } from '../diet'
import { compact, percent, splitCompact, t } from '../i18n'
import { Avatar } from './Avatar'

const RULER_SCALE = 1_000_000
const SOURCE_NAMES: Record<Source, string> = { claude: 'Claude Code', codex: 'Codex' }
const DISPLAY: Record<string, string> = { 'your prompts': t.yourPrompts, 'injected text': t.injected }

interface EfficiencyViewProps {
  report: Report
}

export function EfficiencyView({ report }: EfficiencyViewProps) {
  const [source, setSource] = useState<Source>(report.sources[0]?.overview.source ?? 'claude')
  const selected = report.sources.find((s) => s.overview.source === source) ?? report.sources[0]
  const lines = selected ? dietLines(selected) : []
  const best = Math.max(0, ...report.sources.flatMap((s) => s.suggestions.map((x) => x.saving_share)))
  const count = report.sources.reduce((sum, s) => sum + s.suggestions.length, 0)
  const status = report.context_status
  return (
    <main className="grade eficiencia">
      <AgentCard agent={AGENTS.raio} value={status ? status.context_tokens : 0} label={t.raioLine} />
      <AgentCard agent={AGENTS.dieta} value={best} label={`${t.dietaLine} · ${count} ${t.suggestions}`} isShare />
      <AgentCard agent={AGENTS.memo} value={0} label={t.memoLine} />
      <section className="cartao dieta-lista">
        <h2>
          {t.dietaTitle}
          <small>{t.dietaHint}</small>
          <span className="seletor" role="tablist">
            {report.sources.map((s) => (
              <button
                key={s.overview.source}
                role="tab"
                aria-selected={s.overview.source === source}
                className={s.overview.source === source ? 'ativo' : ''}
                onClick={() => setSource(s.overview.source)}
              >
                {SOURCE_NAMES[s.overview.source]}
              </button>
            ))}
          </span>
        </h2>
        <ol className="sugestoes">
          {lines.map((line) => (
            <li key={line.id}>
              <div className="linha-1">
                <span className="nome">{line.title}</span>
                <span className="num">{line.saving > 0 ? `-${compact(line.saving)}` : ''}</span>
                <span className="apoio">{line.share > 0 ? `${line.isUpperBound ? t.upTo : '~'}${percent(line.share)}` : ''}</span>
              </div>
              <div className="linha-2">{line.evidence}</div>
              <div className="linha-3">{line.how}</div>
            </li>
          ))}
        </ol>
      </section>
      {status && <SessionCard status={status} />}
    </main>
  )
}

interface AgentCardProps {
  agent: Agent
  value: number
  label: string
  isShare?: boolean
}

function AgentCard({ agent, value, label, isShare = false }: AgentCardProps) {
  const big = isShare ? { value: percent(value), unit: '' } : splitCompact(value)
  return (
    <section className="cartao agente">
      <div className="agente-topo">
        <Avatar agent={agent} size={44} />
        <span className="agente-nome">
          <b>{agent.name}</b>
          <span>{agent.role}</span>
        </span>
      </div>
      <div className="destaque">
        <strong>
          {isShare && value > 0 ? `${t.upTo}${big.value}` : big.value}
          {big.unit && <span className="unidade-pequena">{big.unit}</span>}
        </strong>
        <span>{label}</span>
      </div>
    </section>
  )
}

interface SessionCardProps {
  status: ContextStatus
}

/** Raio's view of the current session: where it sits on the ruler and what it loaded. */
function SessionCard({ status }: SessionCardProps) {
  const at = (value: number) => ({ left: `${Math.min(100, (value / RULER_SCALE) * 100)}%` })
  return (
    <section className="cartao sessao">
      <h2>
        {t.sessionTitle}
        <small>
          {SOURCE_NAMES[status.source]} · {status.project.split('/').pop()} · {status.requests_since_compaction} {t.requests}
        </small>
      </h2>
      <div className={`trilho-wrap ${status.source}`}>
        <div className="trilho" />
        {status.usual_compaction_at > 0 && <span className="corte" style={at(status.usual_compaction_at)} />}
        <span className="marcador mediana" style={at(status.context_tokens)} />
        <span className="rotulo-marca acima" style={at(status.context_tokens)}>
          {t.now} {compact(status.context_tokens)}
        </span>
        {status.usual_compaction_at > 0 && (
          <span className="rotulo-marca abaixo antes" style={at(status.usual_compaction_at)}>
            {t.compacts} {compact(status.usual_compaction_at)}
          </span>
        )}
        <span className="rotulo-escala inicio">0</span>
      </div>
      <ul className="lista">
        <li>
          <i className="ponto" />
          <span className="nome">{t.startup}</span>
          <span className="num">{compact(status.startup_tokens)}</span>
          <span className="apoio">{percent(status.startup_tokens / Math.max(1, status.context_tokens))}</span>
        </li>
        {status.loaded_since_compaction.map((item) => (
          <li key={item.name}>
            <i className={`ponto ${status.source}`} />
            <span className="nome">{DISPLAY[item.name] ?? item.name}</span>
            <span className="num">{compact(item.tokens)}</span>
            <span className="apoio">{percent(item.tokens / Math.max(1, status.context_tokens))}</span>
          </li>
        ))}
      </ul>
    </section>
  )
}
