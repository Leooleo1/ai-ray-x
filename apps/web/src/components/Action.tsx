import { useEffect, useRef, useState } from 'react'
import { applySuggestion, undoChange, type Change, type Outcome, type Source } from '../api'
import { explainApply } from '../diet'
import { compact, fill, t } from '../i18n'
import { describeOutcome } from '../outcome'

interface ActionProps {
  id: string
  value?: string | null
  source: Source
  change: Change | undefined
  outcome: Outcome | undefined
  onChanged: () => void
  /** Acts on this bot's project instead of the one `raiox open` started in. */
  bot?: string
}

type Step = { kind: 'idle' } | { kind: 'confirm' } | { kind: 'busy' } | { kind: 'done'; message: string } | { kind: 'failed'; message: string }

/** Two taps to apply (Apply, then Confirm); one to undo. The server keeps a backup and a ledger. */
export function Action({ id, value, source, change, outcome, onChanged, bot }: ActionProps) {
  const [step, setStep] = useState<Step>({ kind: 'idle' })
  const box = useRef<HTMLDivElement>(null)
  // The control grows when it asks for confirmation; keep it in view (the chat scrolls).
  useEffect(() => {
    if (step.kind !== 'idle') box.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  }, [step.kind])
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
      <div className="acao" ref={box}>
        <span className="estado" title={`${change.key}=${change.after}`}>
          <i className="ponto ok" />
          {t.applied} · {settingLabel(change)}
        </span>
        <button className="link" disabled={step.kind === 'busy'} onClick={() => void run(() => undoChange(change.id, bot), () => t.undo)}>
          {t.undo}
        </button>
        {outcome && <span className={`nota resultado ${outcome.conclusive ? 'medido' : ''}`}>{describeOutcome(outcome)}</span>}
        {note}
      </div>
    )
  }
  if (step.kind === 'confirm') {
    return (
      <div className="acao" ref={box}>
        <button className="cheio" onClick={() => void run(() => applySuggestion(id, source, bot), (kept) => (kept ? t.memoSaved : t.memoFailed))}>
          {t.confirm}
        </button>
        <button className="link" onClick={() => setStep({ kind: 'idle' })}>
          {t.cancel}
        </button>
        <span className="nota explica">{fill(t.applyTarget, id === 'compact-earlier' ? `CLAUDE_CODE_AUTO_COMPACT_WINDOW=${value ?? '400000'}` : id)}</span>
      </div>
    )
  }
  const tipId = `dica-${id}`
  return (
    <div className="acao" ref={box}>
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
          {explainApply(id, value).map((line) => (
            <span key={line}>{line}</span>
          ))}
        </span>
      </span>
      {note}
    </div>
  )
}

/** A setting in words; the raw key stays in the tooltip. */
function settingLabel(change: Change): string {
  if (change.key === 'CLAUDE_CODE_AUTO_COMPACT_WINDOW') return `${t.compacts} ${compact(Number(change.after))}`
  return `${change.key}=${change.after}`
}
