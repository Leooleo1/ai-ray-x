import { type CSSProperties, useEffect, useState } from 'react'
import { type Face, faceOf, stageLight } from '../avatars'
import { PRESETS } from '../onboarding/presets'
import { REDUCED_MOTION } from './parts'
import { type Cast, SCENES } from './scenes'
import { tt } from './texts'
import './tour.css'

/** Two ready-made bots to play the other projects, in colors unlike the person's. */
function othersFor(face: Face): [Face, Face] {
  const [a, b] = PRESETS.filter((p) => p.color !== face.color).map((p) => faceOf(p.shape, p.color, p.name))
  return [a, b]
}

interface TourProps {
  face: Face
  agent: string
}

/** The mini tutorial: five short animated scenes with the person's bot, one after another. */
export function Tour({ face, agent }: TourProps) {
  const [index, setIndex] = useState(0)
  // Bumped to replay a scene that is already showing.
  const [run, setRun] = useState(0)
  const cast: Cast = { face, agent, others: othersFor(face) }
  const { Scene, duration } = SCENES[index]
  const isLast = index === SCENES.length - 1
  useEffect(() => {
    if (REDUCED_MOTION || isLast) return undefined
    const timer = window.setTimeout(() => setIndex(index + 1), duration)
    return () => window.clearTimeout(timer)
  }, [index, run, duration, isLast])
  const show = (next: number) => {
    setIndex(next)
    setRun((n) => n + 1)
  }
  const [title, text] = tt.scenes[index]
  return (
    <div className="tour" aria-label={tt.label}>
      <div className="tour-trilho" role="tablist" aria-label={tt.label}>
        {SCENES.map((scene, i) => (
          <button
            key={tt.scenes[i][0]}
            type="button"
            role="tab"
            aria-selected={i === index}
            aria-label={tt.scenes[i][0]}
            className={i < index ? 'visto' : i === index ? 'agora' : ''}
            style={{ '--dura': `${scene.duration}ms` } as CSSProperties}
            onClick={() => show(i)}
          >
            <i key={i === index ? run : undefined} />
          </button>
        ))}
      </div>
      <div className="tour-palco" style={{ '--luz': `url(${stageLight(face.color)})` } as CSSProperties}>
        <Scene key={`${index}-${run}`} cast={cast} />
      </div>
      <div className="tour-legenda">
        <span className="tour-num">{String(index + 1).padStart(2, '0')}</span>
        <span className="tour-texto">
          <b>{title}</b>
          <small>{text}</small>
        </span>
        <span className="tour-setas">
          <button type="button" aria-label={tt.previous} title={tt.previous} disabled={index === 0} onClick={() => show(index - 1)}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M15 6l-6 6 6 6" />
            </svg>
          </button>
          <button type="button" aria-label={isLast ? tt.replay : tt.next} title={isLast ? tt.replay : tt.next} onClick={() => show(isLast ? 0 : index + 1)}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d={isLast ? 'M4 12a8 8 0 1 0 2.3-5.6M4 4v4h4' : 'M9 6l6 6-6 6'} />
            </svg>
          </button>
        </span>
      </div>
    </div>
  )
}
