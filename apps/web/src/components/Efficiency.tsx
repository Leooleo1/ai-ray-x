import { useEffect, useState } from 'react'
import { AGENTS, type Agent } from '../agents'
import { type ContextStatus, type Report, type Source, fetchSession } from '../api'
import { dietLines } from '../diet'
import { compact, percent, splitCompact, t } from '../i18n'
import { Action } from './Action'
import { Avatar } from './Avatar'
import { CompactionBreakdown } from './Breakdown'
import { Segmented } from './Segmented'

const RULER_SCALE = 1_000_000
/** Below this share of the ruler the 'now' label starts at its mark instead of centering on it. */
const NEAR_START = 0.08
const SOURCE_NAMES: Record<Source, string> = { claude: 'Claude Code', codex: 'Codex' }
const DISPLAY: Record<string, string> = { 'your prompts': t.yourPrompts, 'injected text': t.injected }

interface EfficiencyViewProps {
  report: Report
  onChanged: () => void
}

/** Where a cut can go: the project raiox was opened in, and every project with a bot. */
function targetsOf(report: Report): { dir: string; name: string; bot?: string }[] {
  const here = { dir: report.project_dir, name: report.project_dir.split('/').pop() ?? report.project_dir }
  const others = [...new Map(report.bots.bots.filter((b) => b.dir !== report.project_dir).map((b) => [b.dir, { dir: b.dir, name: b.name, bot: b.id }])).values()]
  return [here, ...others.sort((a, b) => a.name.localeCompare(b.name))]
}

export function EfficiencyView({ report, onChanged }: EfficiencyViewProps) {
  const [source, setSource] = useState<Source>(report.sources[0]?.overview.source ?? 'claude')
  const targets = targetsOf(report)
  const [targetDir, setTargetDir] = useState(targets[0].dir)
  const target = targets.find((x) => x.dir === targetDir) ?? targets[0]
  const inTarget = (file: string) => file.startsWith(`${target.dir}/`)
  const selected = report.sources.find((s) => s.overview.source === source) ?? report.sources[0]
  const lines = selected ? dietLines(selected) : []
  const best = Math.max(0, ...report.sources.flatMap((s) => s.suggestions.map((x) => x.saving_share)))
  const count = report.sources.reduce((sum, s) => sum + s.suggestions.length, 0)
  // The session of the project the cuts go to; undefined while it loads, null when it has none.
  const status = useProjectSession(target.bot)
  const open = report.changes.filter((c) => c.undone_ms === null && inTarget(c.file))
  const lessons = report.changes.filter((c) => c.lesson !== null).length
  const applicable = report.applicable[source] ?? []
  return (
    <main className="grade eficiencia">
      <AgentCard agent={AGENTS.dieta} value={best} label={`${t.dietaLine} · ${count} ${t.suggestions}`} isShare isHero />
      <AgentCard agent={AGENTS.raio} value={status ? status.context_tokens : 0} label={t.raioLine.replace('{p}', target.name)} />
      <AgentCard agent={AGENTS.memo} value={lessons} label={t.memoLine} />
      <section className="cartao dieta-lista">
        <h2>
          {t.dietaTitle}
          <small>{t.dietaHint}</small>
          <Segmented
            label={t.dietaTitle}
            value={source}
            onChange={setSource}
            options={report.sources.map((s) => ({ id: s.overview.source, label: SOURCE_NAMES[s.overview.source] }))}
          />
          {targets.length > 1 && (
            <label className="alvo-projeto">
              <small>{t.applyIn}</small>
              <select value={target.dir} onChange={(event) => setTargetDir(event.target.value)}>
                {targets.map((x) => (
                  <option key={x.dir} value={x.dir}>
                    {x.name}
                  </option>
                ))}
              </select>
            </label>
          )}
        </h2>
        <ol className="sugestoes">
          {lines.map((line) => (
            <li key={line.id}>
              <div className="linha-1">
                <span className="nome">{line.title}</span>
                {line.saving > 0 && <span className="num">{`-${compact(line.saving)}`}</span>}
                {line.share > 0 && <span className="apoio">{`~${percent(line.share)}`}</span>}
              </div>
              {line.id === 'compact-earlier' && selected?.efficiency.compaction.best ? (
                <CompactionBreakdown model={selected.efficiency.compaction} />
              ) : (
                <div className="linha-2">{line.evidence}</div>
              )}
              <div className="linha-3">{line.how}</div>
              {applicable.includes(line.id) && (
                <Action
                  id={line.id}
                  value={line.value}
                  source={source}
                  change={open.find((c) => c.suggestion === line.id)}
                  outcome={report.outcomes.find((o) => o.change_id === open.find((c) => c.suggestion === line.id)?.id)}
                  onChanged={onChanged}
                  bot={target.bot}
                  project={target.name}
                />
              )}
            </li>
          ))}
        </ol>
      </section>
      {status ? (
        <SessionCard status={status} />
      ) : (
        <section className="cartao sessao">
          <h2>{t.sessionTitle}</h2>
          <p className="nota">{status === undefined ? t.probing : t.noSessionIn.replace('{p}', target.name)}</p>
        </section>
      )}
    </main>
  )
}

/** Asks for a project's current session whenever the chosen project changes. */
function useProjectSession(bot: string | undefined): ContextStatus | null | undefined {
  const [status, setStatus] = useState<ContextStatus | null | undefined>(undefined)
  useEffect(() => {
    let alive = true
    setStatus(undefined)
    fetchSession(bot)
      .then((next) => alive && setStatus(next))
      .catch(() => alive && setStatus(null))
    return () => {
      alive = false
    }
  }, [bot])
  return status
}

interface AgentCardProps {
  agent: Agent
  value: number
  label: string
  isShare?: boolean
  /** The tab's one big number: what Dieta could save. */
  isHero?: boolean
}

function AgentCard({ agent, value, label, isShare = false, isHero = false }: AgentCardProps) {
  const big = isShare ? { value: percent(value), unit: '' } : splitCompact(value)
  return (
    <section className={`cartao agente ${isHero ? 'agente-heroi' : ''}`}>
      <div className="agente-topo">
        <Avatar agent={agent} size={isHero ? 40 : 28} followPointer={isHero} state={isHero ? 'idle' : 'still'} />
        <b className="agente-nome">{agent.name}</b>
      </div>
      <div className="destaque">
        <strong>
          {isShare && value > 0 && !isHero ? `~${big.value}` : big.value}
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
        <small>{`${status.project.split('/').pop()} · ${status.requests_since_compaction} ${t.requests}`}</small>
      </h2>
      <div className={`trilho-wrap ${status.source}`}>
        <div className="trilho" />
        {status.usual_compaction_at > 0 && <span className="corte" style={at(status.usual_compaction_at)} />}
        <span className="marcador mediana" style={at(status.context_tokens)} />
        <span className={`rotulo-marca acima ${status.context_tokens / RULER_SCALE < NEAR_START ? 'do-inicio' : ''}`} style={at(status.context_tokens)}>
          {t.now} {compact(status.context_tokens)}
        </span>
        {status.usual_compaction_at > 0 && (
          <span className="rotulo-marca abaixo antes" style={at(status.usual_compaction_at)}>
            {t.compacts} {compact(status.usual_compaction_at)}
          </span>
        )}
        <span className="rotulo-escala inicio">0</span>
      </div>
      <ul className="lista sem-ponto">
        <li>
          <span className="nome">{t.startup}</span>
          <span className="num">{compact(status.startup_tokens)}</span>
          <span className="apoio">{percent(status.startup_tokens / Math.max(1, status.context_tokens))}</span>
        </li>
        {status.loaded_since_compaction.map((item) => (
          <li key={item.name}>
            <span className="nome">{DISPLAY[item.name] ?? item.name}</span>
            <span className="num">{compact(item.tokens)}</span>
            <span className="apoio">{percent(item.tokens / Math.max(1, status.context_tokens))}</span>
          </li>
        ))}
      </ul>
    </section>
  )
}
