import type { ReactNode } from 'react'

export type NodeState = 'idle' | 'running' | 'waiting' | 'done' | 'failed'

interface FlowNodeProps {
  icon: ReactNode
  /** Small, above the title: "Trigger", "Step 2". */
  label: string
  title: string
  /** What is missing, said in the node until it is filled. */
  missing?: string | null
  state?: NodeState
  open: boolean
  onToggle: () => void
  /** The editor, shown when open. */
  children?: ReactNode
  /** Under the title: the last run's result, or the approval buttons. */
  footer?: ReactNode
  onRemove?: () => void
  removeLabel?: string
}

/** One node of the chain: its icon, what it is, and its editor when open. */
export function FlowNode({ icon, label, title, missing, state = 'idle', open, onToggle, children, footer, onRemove, removeLabel }: FlowNodeProps) {
  return (
    <section className={`fluxo-no ${state} ${open ? 'aberto' : ''} ${missing ? 'incompleto' : ''}`}>
      <div className="fluxo-no-topo">
        <button type="button" className="fluxo-no-cabeca" aria-expanded={open} onClick={onToggle}>
          <span className="fluxo-no-icone">{icon}</span>
          <span className="fluxo-no-texto">
            <small>{label}</small>
            <b>{title}</b>
          </span>
          {state !== 'idle' && <i className="fluxo-no-estado" aria-hidden="true" />}
        </button>
        {onRemove && (
          <button type="button" className="fluxo-no-tirar" aria-label={removeLabel} title={removeLabel} onClick={onRemove}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M7 7l10 10M17 7L7 17" />
            </svg>
          </button>
        )}
      </div>
      {missing && !open && <p className="fluxo-no-falta">{missing}</p>}
      {open && <div className="fluxo-no-editor">{children}</div>}
      {footer}
    </section>
  )
}
