import { useEffect, useRef, useState } from 'react'
import { t } from '../../i18n'

interface DefaultPickerProps {
  label: string
  value: string | null
  options: string[]
  /** Typed instead of picked (Codex and Cursor take any model name). */
  free: boolean
  show: (value: string) => string
  onPick: (value: string | null) => void
}

/** A compact chip that opens one grouped panel: the CLI's default first, then each option, with a mark on the current one. */
export function DefaultPicker({ label, value, options, free, show, onPick }: DefaultPickerProps) {
  const [open, setOpen] = useState(false)
  const box = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return undefined
    const onDown = (event: MouseEvent) => {
      if (!box.current?.contains(event.target as Node)) setOpen(false)
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false)
    }
    document.addEventListener('mousedown', onDown)
    window.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('mousedown', onDown)
      window.removeEventListener('keydown', onKey)
    }
  }, [open])
  const pick = (next: string | null) => {
    if (next !== value) onPick(next)
    setOpen(false)
  }
  return (
    <div className="cfg-escolha" ref={box}>
      <button type="button" className={`cfg-escolha-botao ${open ? 'aberto' : ''}`} aria-expanded={open} aria-haspopup="dialog" onClick={() => setOpen(!open)}>
        <small>{label}</small>
        <b>{value ? show(value) : t.defaultOption}</b>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M7 10l5 5 5-5" />
        </svg>
      </button>
      {open && (
        <div className="cfg-escolha-painel" role="dialog" aria-label={label}>
          <span className="cfg-escolha-titulo">{label}</span>
          {free ? (
            <FreeValue value={value} options={options} show={show} onPick={pick} />
          ) : (
            <div className="cfg-escolha-grade" role="radiogroup" aria-label={label}>
              {[null, ...options].map((option) => (
                <button key={option ?? ''} type="button" role="radio" aria-checked={value === option} className={value === option ? 'ativo' : ''} onClick={() => pick(option)}>
                  <i className="cfg-marca" aria-hidden="true" />
                  {option ? show(option) : t.defaultOption}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  )
}

/** Any model name, or the CLI's default when left empty. */
function FreeValue({ value, options, show, onPick }: { value: string | null; options: string[]; show: (value: string) => string; onPick: (value: string | null) => void }) {
  const [draft, setDraft] = useState(value ?? '')
  useEffect(() => {
    setDraft(value ?? '')
  }, [value])
  const apply = () => onPick(draft.trim() || null)
  return (
    <div className="cfg-escolha-livre-wrap">
      <form
        className="cfg-escolha-livre"
        onSubmit={(event) => {
          event.preventDefault()
          apply()
        }}
      >
        {/* The panel opens on a click, so the field takes the focus right away. */}
        <input autoFocus value={draft} placeholder={t.defaultOption} onChange={(event) => setDraft(event.target.value)} />
        <button type="submit">{t.applyValue}</button>
      </form>
      {options.length > 0 && (
        <div className="cfg-escolha-grade" role="radiogroup">
          {[null, ...options].map((option) => (
            <button key={option ?? ''} type="button" role="radio" aria-checked={value === option} className={value === option ? 'ativo' : ''} onClick={() => onPick(option)}>
              <i className="cfg-marca" aria-hidden="true" />
              {option ? show(option) : t.defaultOption}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
