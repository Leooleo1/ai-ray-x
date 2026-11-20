import type { ReactNode } from 'react'

export type NodeState = 'idle' | 'running' | 'waiting' | 'done' | 'failed'

interface FlowNodeProps {
  icon: ReactNode
  /** Read aloud only: "Trigger", "Step 2" (the order is already on the line). */
  label: string
  title: string
  /** What is missing, said under the title until it is filled. */
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
  /** The last node has no line going on below it. */
  last?: boolean
}

/** One stop on the line: its icon sits on the line, its title and result beside it, its editor below. */
export function FlowNode({ icon, label, title, missing, state = 'idle', open, onToggle, children, footer, onRemove, removeLabel, last = false }: FlowNodeProps) {
  return (
    <section className={`fluxo-no ${state} ${open ? 'aberto' : ''} ${last ? 'ultimo' : ''}`} aria-label={`${label}: ${title}`}>
      <span className="fluxo-no-icone">{icon}</span>
      <div className="fluxo-no-corpo">
        <div className="fluxo-no-topo">
          <button type="button" className="fluxo-no-cabeca" aria-expanded={open} onClick={onToggle}>
            {title}
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
        {footer}
        {open && <div className="fluxo-no-editor">{children}</div>}
      </div>
    </section>
  )
}
