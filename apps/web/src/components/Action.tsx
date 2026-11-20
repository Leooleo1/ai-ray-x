import { useEffect, useId, useRef, useState } from 'react'
import { applySuggestion, undoChange, type Change, type Outcome, type Source } from '../api'
import { explainApply, settingKeyOf } from '../diet'
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
  /** The project the change goes to, named in the confirmation. */
  project: string
  /** Optional custom button text when idle. */
  label?: string
  /** Highlight as primary CTA when appropriate. */
  primary?: boolean
}

type Step = { kind: 'idle' } | { kind: 'confirm' } | { kind: 'confirm_switch' } | { kind: 'busy' } | { kind: 'done'; message: string } | { kind: 'failed'; message: string }

/** Two taps to apply (Apply, then Confirm); one to undo. The server keeps a backup and a ledger. */
export function Action({ id, value, source, change, outcome, onChanged, bot, project, label, primary = false }: ActionProps) {
  const [step, setStep] = useState<Step>({ kind: 'idle' })
  const box = useRef<HTMLDivElement>(null)
  const tipId = useId()
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
  const keyName = settingKeyOf(id) ?? id
  const desired = value ?? (id === 'compact-earlier' ? '400000' : id === 'trim-tool-output' ? '15000' : '')
  const canSwitch = Boolean(change && desired && change.after !== desired)
  const note = step.kind === 'done' || step.kind === 'failed' ? <span className={`nota ${step.kind}`}>{step.message}</span> : null

  if (change) {
    if (step.kind === 'confirm_switch') {
      const switchValue = async () => {
        await undoChange(change.id, bot)
        return applySuggestion(id, source, bot, desired)
      }
      return (
        <div className="acao" ref={box}>
          <button type="button" className="cheio" onClick={() => void run(switchValue, (kept) => (kept ? t.memoSaved : t.memoFailed))}>
            {t.confirm}
          </button>
          <button type="button" className="link" onClick={() => setStep({ kind: 'idle' })}>
            {t.cancel}
          </button>
          <span className="nota explica">{fill(t.applyTarget, `${keyName}=${desired}`).replace('{project}', project)}</span>
        </div>
      )
    }
    return (
      <div className="acao" ref={box}>
        <span className="estado" title={`${change.key}=${change.after}`}>
          <i className="ponto ok" />
          {t.applied} · {settingLabel(change)}
        </span>
        {canSwitch && (
          <button type="button" className="vidro-btn" disabled={step.kind === 'busy'} onClick={() => setStep({ kind: 'confirm_switch' })}>
            {t.efiSwitchTo.replace('{v}', valueLabel(id, desired))}
          </button>
        )}
        <button type="button" className="link" disabled={step.kind === 'busy'} onClick={() => void run(() => undoChange(change.id, bot), () => t.deliveryUndone)}>
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
        <button type="button" className="cheio" onClick={() => void run(() => applySuggestion(id, source, bot, desired || undefined), (kept) => (kept ? t.memoSaved : t.memoFailed))}>
          {t.confirm}
        </button>
        <button type="button" className="link" onClick={() => setStep({ kind: 'idle' })}>
          {t.cancel}
        </button>
        <span className="nota explica">{fill(t.applyTarget, desired ? `${keyName}=${desired}` : id).replace('{project}', project)}</span>
      </div>
    )
  }
  const tips = explainApply(id, desired || value)
  return (
    <div className="acao" ref={box}>
      <span className="dica-wrap">
        <button
          type="button"
          className={primary ? 'cheio efi-btn-primario' : 'vidro-btn'}
          aria-describedby={tips.length > 0 ? tipId : undefined}
          disabled={step.kind === 'busy'}
          onClick={() => setStep({ kind: 'confirm' })}
        >
          {label ?? t.apply}
        </button>
        {tips.length > 0 && (
          <span className="dica" role="tooltip" id={tipId}>
            {tips.map((line) => (
              <span key={line}>{line}</span>
            ))}
          </span>
        )}
      </span>
      {note}
    </div>
  )
}

function valueLabel(id: string, raw: string): string {
  const num = Number(raw)
  if (!Number.isFinite(num) || num <= 0) return raw
  if (id === 'trim-tool-output') return `${compact(num)} ${t.efiCharsShort}`
  return compact(num)
}

/** A setting in words; the raw key stays in the tooltip. */
export function settingLabel(change: Change): string {
  if (change.key === 'CLAUDE_CODE_AUTO_COMPACT_WINDOW') return `${t.compacts} ${compact(Number(change.after))}`
  if (change.key === 'BASH_MAX_OUTPUT_LENGTH') return `${t.efiOutputLimitShort} ${compact(Number(change.after))}`
  return `${change.key}=${change.after}`
}
