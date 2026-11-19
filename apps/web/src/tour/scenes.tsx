// The tutorial's five scenes. Each is a timeline of beats over the panel's
// own pieces, with the person's bot in the lead role.

import { type ReactNode, useRef } from 'react'
import type { Face } from '../avatars'
import { Avatar } from '../components/Avatar'
import { Cursor, From, Rich, Typed, Window, useBeats } from './parts'
import { tt } from './texts'
import './scenes.css'

export interface Cast {
  /** The person's bot. */
  face: Face
  /** Its CLI, as the panel names it ("Claude Code"). */
  agent: string
  /** Two other bots for the scenes with several projects. */
  others: [Face, Face]
}

interface SceneProps {
  cast: Cast
}

/** Time a scene stays on its last beat before the next one starts. */
const HOLD_MS = 2200

function Composer({ text }: { text: ReactNode }) {
  return (
    <div className="tour-compor" data-alvo="compor">
      <span>{text || tt.composer}</span>
      <i aria-hidden="true" />
    </div>
  )
}

function Activity({ cast, doing, target }: { cast: Cast; doing: string; target: string }) {
  return (
    <p className="tour-atividade">
      <Avatar agent={cast.face} size={14} state="thinking" />
      <b style={{ color: cast.face.color }}>{cast.agent}</b>
      {doing}
      {target && <code>{target}</code>}
      <span className="digitando" aria-hidden="true">
        <i />
        <i />
        <i />
      </span>
    </p>
  )
}

function Switch({ on, alvo }: { on: boolean; alvo: string }) {
  return <i className={`tour-chave ${on ? 'ligada' : ''}`} data-alvo={alvo} aria-hidden="true" />
}

const BEATS_ASK = [300, 1900, 2600, 3300, 4000, 4800]

function SceneAsk({ cast }: SceneProps) {
  const stage = useRef<HTMLDivElement>(null)
  const beat = useBeats(BEATS_ASK)
  const steps: [string, string][] = [
    [tt.reading, ''],
    [tt.step.list, '**/*'],
    [tt.step.read, tt.file1],
    [tt.step.search, tt.term1],
  ]
  const [doing, target] = steps[Math.min(Math.max(beat - 2, 0), steps.length - 1)]
  return (
    <div className="tour-cena" ref={stage}>
      <Window face={cast.face} project={tt.shop} agent={cast.agent} composer={<Composer text={beat === 1 ? <Typed text={tt.ask1} active /> : ''} />}>
        <From beat={beat} at={2} className="tour-minha">
          {tt.ask1}
        </From>
        <From beat={beat} at={6} className="tour-resposta">
          <p>
            <Rich text={tt.answer1} />
          </p>
          <small>{tt.meta1}</small>
        </From>
        {beat >= 2 && beat < 6 && <Activity cast={cast} doing={doing} target={target} />}
      </Window>
      <Cursor stage={stage} target={beat === 1 ? 'compor' : null} pressed={false} />
    </div>
  )
}

const BEATS_APPROVE = [900, 1900, 2300, 3100, 3900]

function SceneApprove({ cast }: SceneProps) {
  const stage = useRef<HTMLDivElement>(null)
  const beat = useBeats(BEATS_APPROVE)
  const editing = beat === 3 || beat === 4 ? tt.files2[beat - 3] : null
  return (
    <div className="tour-cena" ref={stage}>
      <Window face={cast.face} project={tt.shop} agent={cast.agent} composer={<Composer text="" />}>
        <div className="tour-minha dentro tour-surge">{tt.ask2}</div>
        <div className="tour-resposta dentro tour-surge">
          <p>{tt.answer2}</p>
          <div className="tour-proposta">
            <small>{tt.proposal}</small>
            {tt.files2.map((file) => (
              <code key={file}>{file}</code>
            ))}
            <button type="button" tabIndex={-1} className={beat >= 2 ? 'feito' : ''} data-alvo="aprovar">
              {beat >= 5 ? tt.approved : tt.approve}
            </button>
          </div>
        </div>
        {editing && <Activity cast={cast} doing={tt.step.edit} target={editing} />}
      </Window>
      <Cursor stage={stage} target={beat >= 1 && beat <= 2 ? 'aprovar' : null} pressed={beat === 2} />
    </div>
  )
}

const BEATS_MANY = [500, 1200, 1600, 3200, 3900, 4600]
const PROJECTS = [tt.shop, 'api', 'site']

function SceneMany({ cast }: SceneProps) {
  const stage = useRef<HTMLDivElement>(null)
  const beat = useBeats(BEATS_MANY)
  const faces = [cast.face, ...cast.others]
  return (
    <div className="tour-cena tour-varios" ref={stage}>
      <div className="tour-pergunta">
        <span className="tour-rostos">
          {faces.map((face, i) => (
            <Avatar key={PROJECTS[i]} agent={face} size={18} state="still" />
          ))}
        </span>
        <b>{tt.ask3}</b>
        <button type="button" tabIndex={-1} className={beat >= 2 ? 'feito' : ''} data-alvo="perguntar">
          {tt.askMany}
        </button>
      </div>
      <div className="tour-colunas">
        {faces.map((face, i) => {
          const answered = beat >= 4 + i
          return (
            <From key={PROJECTS[i]} beat={beat} at={3} className="tour-coluna">
              <header>
                <Avatar agent={face} size={16} state={beat >= 3 && !answered ? 'thinking' : 'still'} />
                <b>{PROJECTS[i]}</b>
              </header>
              <p>{answered ? tt.answers3[i] : <span className="tour-passo">{`${tt.step.read} …`}</span>}</p>
              <small>{answered ? tt.read3[i] : ''}</small>
            </From>
          )
        })}
      </div>
      <Cursor stage={stage} target={beat >= 1 && beat <= 2 ? 'perguntar' : null} pressed={beat === 2} />
    </div>
  )
}

const BEATS_WIRE = [600, 1300, 2100, 2800, 3400, 3700, 5100]

function SceneWire({ cast }: SceneProps) {
  const stage = useRef<HTMLDivElement>(null)
  const beat = useBeats(BEATS_WIRE)
  const wired = beat >= 2
  const node = (face: Face, name: string) => (
    <span className="tour-no">
      <Avatar agent={face} size={40} state="still" />
      <b>{name}</b>
    </span>
  )
  return (
    <div className="tour-cena tour-fios" ref={stage}>
      <div className="tour-par">
        {node(cast.face, tt.shop)}
        <span className={`tour-fio ${wired ? 'ligado' : ''}`}>
          <i className={`tour-envelope ${beat >= 6 ? 'viajando' : ''}`}>{tt.via}</i>
        </span>
        {node(cast.others[0], 'api')}
      </div>
      <div className="tour-fio-linha">
        <span>
          loja <small>⇄</small> api
        </span>
        <Switch on={wired} alvo="chave" />
        <small>{wired ? tt.wire : ''}</small>
      </div>
      <From beat={beat} at={3} className="tour-nota">
        <small>{tt.toApi}</small>
        <p>{tt.note4}</p>
        <button type="button" tabIndex={-1} className={beat >= 5 ? 'feito' : ''} data-alvo="enviar">
          {tt.send}
        </button>
      </From>
      <From beat={beat} at={7} className="tour-resposta tour-volta">
        <p>{tt.reply4}</p>
      </From>
      <Cursor stage={stage} target={beat === 1 || beat === 2 ? 'chave' : beat === 4 || beat === 5 ? 'enviar' : null} pressed={beat === 2 || beat === 5} />
    </div>
  )
}

const BEATS_PIN = [600, 1300, 2200, 3000, 3600]

function ScenePin({ cast }: SceneProps) {
  const stage = useRef<HTMLDivElement>(null)
  const beat = useBeats(BEATS_PIN)
  const pinned = beat >= 2
  return (
    <div className="tour-cena" ref={stage}>
      <Window face={cast.face} project={tt.shop} agent={cast.agent}>
        <span className={`tour-fixadas ${pinned ? 'contou' : ''}`}>{`${tt.pins}: ${pinned ? 1 : 0}`}</span>
        <div className="tour-resposta dentro tour-surge">
          <p>
            <Rich text={tt.answer1} />
          </p>
          <span className="tour-acoes">
            <small>{tt.copy}</small>
            <small className={pinned ? 'fixada' : ''} data-alvo="fixar">
              {pinned ? tt.pinned : tt.pin}
            </small>
          </span>
        </div>
        <From beat={beat} at={3} className="tour-rotina">
          <span>
            <small>{tt.routine}</small>
            <b>{tt.routineText}</b>
            <small>{beat >= 5 ? tt.nextRun : ''}</small>
          </span>
          <Switch on={beat >= 5} alvo="rotina" />
        </From>
      </Window>
      <Cursor stage={stage} target={beat === 1 || beat === 2 ? 'fixar' : beat === 4 || beat === 5 ? 'rotina' : null} pressed={beat === 2 || beat === 5} />
    </div>
  )
}

const last = (beats: number[]) => beats[beats.length - 1] + HOLD_MS

export const SCENES = [
  { Scene: SceneAsk, duration: last(BEATS_ASK) },
  { Scene: SceneApprove, duration: last(BEATS_APPROVE) },
  { Scene: SceneMany, duration: last(BEATS_MANY) },
  { Scene: SceneWire, duration: last(BEATS_WIRE) },
  { Scene: ScenePin, duration: last(BEATS_PIN) },
]
