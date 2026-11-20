import { useState } from 'react'
import type { Bot, Channel, FlowAction, FlowRun, FlowStep, FlowTrigger } from '../../api'
import { faceForBot } from '../../avatars'
import { locale, t } from '../../i18n'
import { Avatar } from '../Avatar'
import { stepMissing, stepTitle, triggerLine, triggerMissing } from './describe'
import { FlowNode, type NodeState } from './FlowNode'
import { FlowIcon } from './icons'
import { StepEditor } from './StepEditor'
import { freshTrigger, TriggerEditor } from './TriggerEditor'

export interface Draft {
  id: string
  name: string
  enabled: boolean
  trigger: FlowTrigger | null
  steps: FlowStep[]
}

const NEW_STEPS: { kind: FlowStep['kind']; label: () => string; make: () => FlowStep }[] = [
  { kind: 'ask', label: () => t.flowAddAsk, make: () => ({ kind: 'ask', bot: '', text: '' }) },
  { kind: 'approve', label: () => t.flowAddApprove, make: () => ({ kind: 'approve', text: '' }) },
  { kind: 'post', label: () => t.flowAddPost, make: () => ({ kind: 'post', channel: '', text: '' }) },
]

const clock = (ms: number) => new Date(ms).toLocaleString(locale, { weekday: 'short', hour: '2-digit', minute: '2-digit' }).replace('.', '')

interface FlowCanvasProps {
  draft: Draft
  bots: Bot[]
  channels: Channel[]
  /** The run going on, or else the last one. */
  run: FlowRun | null
  live: boolean
  error: string | null
  onChange: (draft: Draft) => void
  onAction: (action: FlowAction) => void
}

/** The workflow drawn as a chain on a dotted board: its trigger, its steps, and where a run is. */
export function FlowCanvas({ draft, bots, channels, run, live, error, onChange, onAction }: FlowCanvasProps) {
  const [open, setOpen] = useState<string | null>(null)
  const [adding, setAdding] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const toggle = (key: string) => setOpen(open === key ? null : key)
  const setStep = (i: number, step: FlowStep) => onChange({ ...draft, steps: draft.steps.map((s, j) => (j === i ? step : s)) })
  const incomplete = (draft.trigger ? triggerMissing(draft.trigger) : null) ?? draft.steps.map(stepMissing).find(Boolean) ?? null
  const stateOf = (i: number): NodeState => {
    if (!run || run.step < i) return 'idle'
    if (i < run.results.length) return run.results[i].ok ? 'done' : 'failed'
    if (run.step === i && live) return run.status === 'waiting' ? 'waiting' : 'running'
    return 'idle'
  }
  const footerOf = (i: number) => {
    const result = run?.results[i]
    if (result && draft.steps[i]?.kind === 'approve') return <p className="fluxo-no-resultado">{result.ok ? t.flowApproved : t.flowDeclined}</p>
    if (result && (result.text || !result.ok)) return <p className={`fluxo-no-resultado ${result.ok ? '' : 'falhou'}`}>{result.text || t.flowDeclined}</p>
    if (run?.step === i && live && run.status === 'waiting') {
      return (
        <div className="fluxo-no-acoes">
          <button type="button" className="bv-btn cheio" onClick={() => onAction('approve')}>
            {t.flowApproveNow}
          </button>
          <button type="button" className="acao-texto" onClick={() => onAction('decline')}>
            {t.flowStopHere}
          </button>
        </div>
      )
    }
    if (run?.step === i && live) return <p className="fluxo-no-resultado">{run.asked_ms ? t.flowAnswering : t.flowWaitingAi}</p>
    return null
  }
  if (!draft.trigger) {
    return (
      <div className="fluxo-tabuleiro">
        <div className="fluxo-comeco">
          <span className="fluxo-raio">
            <FlowIcon name="trigger" />
          </span>
          <b>{t.flowStartTitle}</b>
          <button
            type="button"
            className="bv-btn cheio"
            onClick={() => {
              onChange({ ...draft, trigger: freshTrigger('manual') })
              setOpen('gatilho')
            }}
          >
            <FlowIcon name="plus" />
            {t.flowAddTrigger}
          </button>
        </div>
      </div>
    )
  }
  return (
    <div className="fluxo-tabuleiro">
      <div className="fluxo-corrente">
        <FlowNode
          icon={<FlowIcon name="trigger" />}
          label={t.flowTrigger}
          title={triggerLine(draft.trigger, bots)}
          missing={triggerMissing(draft.trigger)}
          open={open === 'gatilho'}
          onToggle={() => toggle('gatilho')}
          state={run && live ? 'done' : 'idle'}
          footer={run && run.cause ? <p className="fluxo-no-resultado">{run.cause}</p> : null}
        >
          <TriggerEditor trigger={draft.trigger} bots={bots} onChange={(trigger) => onChange({ ...draft, trigger })} />
        </FlowNode>
        {draft.steps.map((step, i) => {
          const bot = step.kind === 'ask' ? bots.find((b) => b.id === step.bot) : undefined
          return (
            <div key={i} className="fluxo-elo">
              <span className="fluxo-fio" aria-hidden="true" />
              <FlowNode
                icon={bot ? <Avatar agent={faceForBot(bot)} size={22} state={stateOf(i) === 'running' ? 'thinking' : 'still'} /> : <FlowIcon name={step.kind} />}
                label={t.flowStepN.replace('{n}', String(i + 1))}
                title={stepTitle(step, bots, channels)}
                missing={stepMissing(step)}
                state={stateOf(i)}
                open={open === `passo-${i}`}
                onToggle={() => toggle(`passo-${i}`)}
                footer={footerOf(i)}
                removeLabel={t.flowRemoveStep}
                onRemove={live ? undefined : () => onChange({ ...draft, steps: draft.steps.filter((_, j) => j !== i) })}
              >
                <StepEditor step={step} bots={bots} channels={channels} onChange={(next) => setStep(i, next)} />
              </FlowNode>
            </div>
          )
        })}
        {!live && (
          <div className="fluxo-elo">
            <span className="fluxo-fio" aria-hidden="true" />
            {adding ? (
              <div className="fluxo-novo-passo" role="menu">
                {NEW_STEPS.map(({ kind, label, make }) => (
                  <button
                    key={kind}
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      onChange({ ...draft, steps: [...draft.steps, make()] })
                      setOpen(`passo-${draft.steps.length}`)
                      setAdding(false)
                    }}
                  >
                    <FlowIcon name={kind} />
                    {label()}
                  </button>
                ))}
              </div>
            ) : (
              <button type="button" className="fluxo-mais" onClick={() => setAdding(true)}>
                <FlowIcon name="plus" />
                {t.flowAddStep}
              </button>
            )}
          </div>
        )}
      </div>
      <footer className="fluxo-rodape">
        {error ? <p className="nota failed quebra">{error}</p> : incomplete ? <span>{incomplete}</span> : run ? <span>{t.flowLastRun.replace('{s}', t.flowStatus[run.status]).replace('{w}', clock(run.ended_ms ?? run.started_ms))}</span> : <span>{t.flowSaved}</span>}
        {deleting ? (
          <button type="button" className="acao-texto perigo" onBlur={() => setDeleting(false)} onClick={() => onAction('delete')}>
            {t.flowDeleteSure}
          </button>
        ) : (
          <button type="button" className="acao-texto" onClick={() => setDeleting(true)}>
            {t.flowDelete}
          </button>
        )}
      </footer>
    </div>
  )
}
