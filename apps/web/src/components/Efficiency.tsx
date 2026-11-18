import { useState } from 'react'
import { AGENTS, type Agent } from '../agents'
import { applySuggestion, undoChange, type Change, type ContextStatus, type Report, type Source } from '../api'
import { dietLines, explainApply } from '../diet'
import { compact, fill, percent, splitCompact, t } from '../i18n'
import { Avatar } from './Avatar'
import { Segmented } from './Segmented'

const RULER_SCALE = 1_000_000
const SOURCE_NAMES: Record<Source, string> = { claude: 'Claude Code', codex: 'Codex' }
const DISPLAY: Record<string, string> = { 'your prompts': t.yourPrompts, 'injected text': t.injected }

interface EfficiencyViewProps {
  report: Report
  onChanged: () => void
}

export function EfficiencyView({ report, onChanged }: EfficiencyViewProps) {
  const [source, setSource] = useState<Source>(report.sources[0]?.overview.source ?? 'claude')
  const selected = report.sources.find((s) => s.overview.source === source) ?? report.sources[0]
  const lines = selected ? dietLines(selected) : []
  const best = Math.max(0, ...report.sources.flatMap((s) => s.suggestions.map((x) => x.saving_share)))
  const count = report.sources.reduce((sum, s) => sum + s.suggestions.length, 0)
  const status = report.context_status
  const open = report.changes.filter((c) => c.undone_ms === null)
  const lessons = report.changes.filter((c) => c.lesson !== null).length
  const applicable = report.applicable[source] ?? []
  return (
    <main className="grade eficiencia">
      <AgentCard agent={AGENTS.raio} value={status ? status.context_tokens : 0} label={t.raioLine} />
      <AgentCard agent={AGENTS.dieta} value={best} label={`${t.dietaLine} · ${count} ${t.suggestions}`} isShare />
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
              {applicable.includes(line.id) && (
                <Action
                  id={line.id}
                  source={source}
                  change={open.find((c) => c.suggestion === line.id)}
                  onChanged={onChanged}
                />
              )}
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

interface ActionProps {
  id: string
  source: Source
  change: Change | undefined
  onChanged: () => void
}

type Step = { kind: 'idle' } | { kind: 'confirm' } | { kind: 'busy' } | { kind: 'done'; message: string } | { kind: 'failed'; message: string }

/** Two taps to apply (Apply, then Confirm); one to undo. The server keeps a backup and a ledger. */
function Action({ id, source, change, onChanged }: ActionProps) {
  const [step, setStep] = useState<Step>({ kind: 'idle' })
  const run = async (task: () => ReturnType<typeof applySuggestion>, ok: (memoKept: boolean) => string) => {
    setStep({ kind: 'busy' })
    try {
      const result = await task()
      setStep({ kind: 'done', message: ok(Boolean(result.memo?.kept)) })
      onChanged()
    } catch (error: unknown) {
      setStep({ kind: 'failed', message: error instanceof Error ? error.message : String(error) })
    }
  }
  const note = step.kind === 'done' || step.kind === 'failed' ? <span className={`nota ${step.kind}`}>{step.message}</span> : null
  if (change) {
    return (
      <div className="acao">
        <span className="estado">
          <i className="ponto ok" />
          {t.applied} · {change.key}={change.after}
        </span>
        <button className="link" disabled={step.kind === 'busy'} onClick={() => void run(() => undoChange(change.id), () => t.undo)}>
          {t.undo}
        </button>
        {note}
      </div>
    )
  }
  if (step.kind === 'confirm') {
    return (
      <div className="acao">
        <button className="cheio" onClick={() => void run(() => applySuggestion(id, source), (kept) => (kept ? t.memoSaved : t.memoFailed))}>
          {t.confirm}
        </button>
        <button className="link" onClick={() => setStep({ kind: 'idle' })}>
          {t.cancel}
        </button>
        <span className="nota">{fill(t.applyTarget, id === 'compact-earlier' ? 'CLAUDE_CODE_AUTO_COMPACT_WINDOW=400000' : id)}</span>
      </div>
    )
  }
  const tipId = `dica-${id}`
  return (
    <div className="acao">
      <span className="dica-wrap">
        <button
          className="vidro-btn"
          aria-describedby={tipId}
          disabled={step.kind === 'busy'}
          onClick={() => setStep({ kind: 'confirm' })}
        >
          {t.apply}
        </button>
        <span className="dica" role="tooltip" id={tipId}>
          {explainApply(id).map((line) => (
            <span key={line}>{line}</span>
          ))}
        </span>
      </span>
      {note}
    </div>
  )
}
