import { type Face, stageLight } from '../../avatars'
import { t } from '../../i18n'
import { tt } from '../../tour/texts'
import { Avatar } from '../Avatar'

export type StepId = 'ais' | 'github' | 'bots' | 'wires'

export interface SetupStep {
  id: StepId
  label: string
  /** null while it is still being checked. */
  done: boolean | null
}

const NEXT: Record<StepId, string> = { ais: t.nextAis, github: t.nextGithub, bots: t.nextBots, wires: t.nextWires }

interface SetupHeroProps {
  steps: SetupStep[]
  summary: string
  face: Face
  onNext: (step: StepId) => void
  onTour: () => void
  onRedo: () => void
}

/** The setup at a glance, as the onboarding left it: how many steps are done, the trail, and the one thing to do next. */
export function SetupHero({ steps, summary, face, onNext, onTour, onRedo }: SetupHeroProps) {
  const checking = steps.some((s) => s.done === null)
  const left = steps.filter((s) => s.done === false)
  const next = checking ? undefined : left[0]
  const done = steps.filter((s) => s.done === true).length
  const title = checking ? t.probing : left.length === 0 ? t.setupDone : left.length === 1 ? t.stepsLeftOne : t.stepsLeftMany.replace('{n}', String(left.length))
  return (
    <section className="cartao cfg-heroi">
      <div className="cfg-heroi-corpo">
        <div className="cfg-heroi-topo">
          <span className="cfg-progresso" aria-label={checking ? t.probing : `${done}/${steps.length}`}>
            {checking ? '–' : done}
            <small>{`/${steps.length}`}</small>
          </span>
          <span className="cfg-heroi-titulo">
            <h2>{title}</h2>
            <small>{checking ? '' : summary}</small>
          </span>
        </div>
        <ol className="cfg-trilha">
          {steps.map((step) => (
            <li key={step.id} className={step.done ? 'feito' : step === next ? 'proximo' : ''}>
              <i aria-hidden="true">
                {step.done && (
                  <svg viewBox="0 0 24 24">
                    <path d="M6 12.5l4 4 8-9" />
                  </svg>
                )}
              </i>
              <span>{step.label}</span>
            </li>
          ))}
        </ol>
        <div className="cfg-heroi-acoes">
          {next && (
            <button type="button" className="cfg-btn cheio" onClick={() => onNext(next.id)}>
              {NEXT[next.id]}
            </button>
          )}
          <button type="button" className={`cfg-btn ${next ? 'vidro' : 'cheio'}`} onClick={onTour}>
            {tt.label}
          </button>
          <button type="button" className="lado-link" onClick={onRedo}>
            {t.redoSetup}
          </button>
        </div>
      </div>
      <div className="cfg-palco" style={{ backgroundImage: `url(${stageLight(face.color)})` }}>
        <Avatar agent={face} size={88} state={!checking && left.length === 0 ? 'happy' : 'idle'} followPointer />
      </div>
    </section>
  )
}
