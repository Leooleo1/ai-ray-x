import type { Finish } from '../api'
import { COLORS, FINISHES, SHAPE_IDS, TEMPLATES, faceOf } from '../avatars'
import { t } from '../i18n'
import { Avatar } from './Avatar'
import './editor.css'

export interface Look {
  shape: string
  color: string
  finish: Finish
}

interface AvatarEditorProps {
  value: Look
  onChange: (look: Look) => void
}

/** Ready-made looks first, then shape, color and finish one by one. */
export function AvatarEditor({ value, onChange }: AvatarEditorProps) {
  const set = (change: Partial<Look>) => onChange({ ...value, ...change })
  const same = (look: Look) => look.shape === value.shape && look.color === value.color && look.finish === value.finish
  return (
    <div className="aparencia">
      <div className="aparencia-campo">
        <span className="aparencia-rotulo">{t.templatesLabel}</span>
        <div className="aparencia-grade" role="radiogroup" aria-label={t.templatesLabel}>
          {TEMPLATES.map((look) => (
            <button key={`${look.shape}-${look.color}-${look.finish}`} type="button" role="radio" aria-checked={same(look)} className={same(look) ? 'ativo' : ''} onClick={() => onChange(look)}>
              <Avatar agent={faceOf(look.shape, look.color, look.shape, look.finish)} size={34} state="still" />
            </button>
          ))}
        </div>
      </div>
      <div className="aparencia-campo">
        <span className="aparencia-rotulo">{t.shapeLabel}</span>
        <div className="aparencia-grade" role="radiogroup" aria-label={t.shapeLabel}>
          {SHAPE_IDS.map((shape) => (
            <button key={shape} type="button" role="radio" aria-checked={value.shape === shape} aria-label={shape} className={value.shape === shape ? 'ativo' : ''} onClick={() => set({ shape })}>
              <Avatar agent={faceOf(shape, value.color, shape, value.finish)} size={30} state="still" />
            </button>
          ))}
        </div>
      </div>
      <div className="aparencia-linha">
        <div className="aparencia-campo">
          <span className="aparencia-rotulo">{t.colorLabel}</span>
          <div className="aparencia-cores" role="radiogroup" aria-label={t.colorLabel}>
            {COLORS.map((color) => (
              <button key={color} type="button" role="radio" aria-checked={value.color === color} aria-label={color} className={value.color === color ? 'ativo' : ''} style={{ background: color }} onClick={() => set({ color })} />
            ))}
          </div>
        </div>
        <div className="aparencia-campo">
          <span className="aparencia-rotulo">{t.finishLabel}</span>
          <div className="aparencia-segmentos" role="radiogroup" aria-label={t.finishLabel}>
            {FINISHES.map((finish) => (
              <button key={finish} type="button" role="radio" aria-checked={value.finish === finish} className={value.finish === finish ? 'ativo' : ''} onClick={() => set({ finish })}>
                {t.finishNames[finish]}
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  )
}
