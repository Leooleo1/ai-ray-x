import { useEffect } from 'react'
import type { Face } from '../avatars'
import { Tour } from './Tour'
import { tt } from './texts'

interface TourDialogProps {
  face: Face
  agent: string
  onClose: () => void
}

/** The tutorial opened again from the panel, over everything; Esc or a click outside closes it. */
export function TourDialog({ face, agent, onClose }: TourDialogProps) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])
  return (
    <div className="tour-fundo" onClick={onClose}>
      <section className="cartao tour-dialogo" role="dialog" aria-modal="true" aria-label={tt.label} onClick={(event) => event.stopPropagation()}>
        <header>
          <b>{tt.label}</b>
          <button type="button" className="icone-btn" aria-label={tt.close} title={tt.close} onClick={onClose}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M6 6l12 12M18 6L6 18" />
            </svg>
          </button>
        </header>
        <Tour face={face} agent={agent} />
      </section>
    </div>
  )
}
