import type { BotAgent, SetupState } from '../api'
import { faceOf } from '../avatars'
import { Avatar } from '../components/Avatar'
import { AvatarEditor } from '../components/AvatarEditor'
import { CUSTOM, PRESETS, type BotDesign, designFrom } from './presets'
import { Frame } from './parts'
import { o } from './texts'

const MAX_NAME = 32
const MAX_INSTRUCTIONS = 1200

interface StepBotProps {
  setup: SetupState
  design: BotDesign
  projects: string[]
  busy: boolean
  onDesign: (design: BotDesign) => void
  onBack: () => void
  onCreate: () => void
}

export function StepBot({ setup, design, projects, busy, onDesign, onBack, onCreate }: StepBotProps) {
  const custom = design.preset === CUSTOM
  const set = (change: Partial<BotDesign>) => onDesign({ ...design, ...change })
  const names = projects.map((p) => p.split('/').pop() ?? p)
  const createLabel = projects.length > 1 ? o.createMany.replace('{n}', String(projects.length)) : o.createOne
  return (
    <Frame
      actions={
        <>
          <button className="bv-btn link" onClick={onBack}>
            {o.back}
          </button>
          <span className="bv-mono bv-destino">{`${o.goesTo} ${names.join(' · ')}`}</span>
          <button className="bv-btn cheio bv-principal" disabled={busy || projects.length === 0} onClick={onCreate}>
            {busy ? o.creating : createLabel}
          </button>
        </>
      }
    >
      <div className="bv-campo">
        <span className="bv-rotulo">{o.runsOn}</span>
        <div className="bv-segmentos" role="radiogroup" aria-label={o.runsOn}>
          {setup.ais.filter((ai) => ai.bots && ai.enabled).map((ai) => (
            <button
              key={ai.id}
              role="radio"
              aria-checked={design.agent === ai.id}
              className={design.agent === ai.id ? 'ativo' : ''}
              disabled={ai.logged_in !== true}
              onClick={() => set({ agent: ai.id as BotAgent })}
            >
              {ai.name}
            </button>
          ))}
        </div>
      </div>

      <div className="bv-presets" role="radiogroup" aria-label={o.botTitle}>
        {PRESETS.map((preset) => (
          <button
            key={preset.id}
            role="radio"
            aria-checked={design.preset === preset.id}
            className={`bv-preset ${design.preset === preset.id ? 'ativo' : ''}`}
            onClick={() => onDesign(designFrom(preset, design.agent))}
          >
            <Avatar agent={faceOf(preset.shape, preset.color, preset.name)} size={36} state="still" />
            <b>{preset.name}</b>
            <span>{preset.role}</span>
          </button>
        ))}
        <button
          role="radio"
          aria-checked={custom}
          className={`bv-preset ${custom ? 'ativo' : ''}`}
          onClick={() => set({ preset: CUSTOM, name: custom ? design.name : o.defaultName, instructions: custom ? design.instructions : '' })}
        >
          <span className="bv-mais-icone" aria-hidden="true">
            +
          </span>
          <b>{o.createOwn}</b>
          <span>{o.createOwnRole}</span>
        </button>
      </div>

      {custom && (
        <div className="bv-criar">
          <label className="bv-campo">
            <span className="bv-rotulo">{o.nameLabel}</span>
            <input className="bv-busca" value={design.name} maxLength={MAX_NAME} onChange={(event) => set({ name: event.target.value })} />
          </label>
          <AvatarEditor value={{ shape: design.shape, color: design.color, finish: design.finish }} onChange={(look) => set(look)} />
          <label className="bv-campo">
            <span className="bv-rotulo">{o.instructionsLabel}</span>
            <textarea className="bv-texto" rows={3} value={design.instructions} maxLength={MAX_INSTRUCTIONS} placeholder={o.instructionsHint} onChange={(event) => set({ instructions: event.target.value })} />
          </label>
        </div>
      )}
    </Frame>
  )
}
