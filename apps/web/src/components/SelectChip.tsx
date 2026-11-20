import { type ReactNode, useEffect, useRef, useState } from 'react'

export interface SelectOption {
  value: string
  label: string
  meta?: string
  icon?: ReactNode
}

interface SelectChipProps {
  label: string
  prefix?: string
  value: string
  options: SelectOption[]
  onChange: (value: string) => void
  align?: 'left' | 'right'
  direction?: 'down' | 'up'
  className?: string
}

/** Custom glass selector replacing native <select>: click-outside, Escape, and optional icons. */
export function SelectChip({ label, prefix, value, options, onChange, align = 'left', direction = 'down', className = '' }: SelectChipProps) {
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
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('mousedown', onDown)
      document.removeEventListener('keydown', onKey)
    }
  }, [open])

  const selected = options.find((o) => o.value === value) ?? options[0]

  return (
    <div className={`seletor-chip ${className}`.trim()} ref={box}>
      <button
        type="button"
        className={`seletor-botao ${open ? 'aberto' : ''}`}
        aria-label={label}
        aria-expanded={open}
        aria-haspopup="listbox"
        onClick={() => setOpen((was) => !was)}
      >
        {prefix && <small className="seletor-prefixo">{prefix}</small>}
        {selected?.icon}
        <b className="seletor-atual">{selected ? selected.label : ''}</b>
        <svg className="seletor-seta" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M7 10l5 5 5-5" />
        </svg>
      </button>
      {open && (
        <ul className={`seletor-menu ${align === 'right' ? 'direita' : ''} ${direction === 'up' ? 'acima' : ''}`} role="listbox" aria-label={label}>
          {options.map((option) => {
            const active = option.value === value
            return (
              <li key={option.value} role="option" aria-selected={active}>
                <button
                  type="button"
                  className={active ? 'ativo' : ''}
                  onClick={() => {
                    onChange(option.value)
                    setOpen(false)
                  }}
                >
                  {option.icon}
                  <span className="seletor-texto">
                    <b>{option.label}</b>
                    {option.meta && <small>{option.meta}</small>}
                  </span>
                  <i className={`seletor-marca ${active ? 'marcado' : ''}`} aria-hidden="true" />
                </button>
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}
