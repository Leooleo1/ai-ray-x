import { useEffect, useRef, useState } from 'react'
import { t } from '../../i18n'
import { type AccessMode, MODES } from '../../modes'

/** One glyph per mode: more arrows, more freedom; the last one is a warning. */
const ICONS: Record<AccessMode, string> = {
  allow: 'M9 6l7 6-7 6z',
  approve: 'M5 12.5l4.5 4.5L19 7.5',
  auto: 'M4 6l7 6-7 6zM12 6l7 6-7 6z',
  full: 'M12 3l10 18H2zM12 10v5M12 17.5v.5',
}

interface ModeChipProps {
  mode: AccessMode
  onPick: (mode: AccessMode) => void
}

/** The bot's mode beside Send: a click opens the four modes to pick one; Shift+Tab in the field steps through them. */
export function ModeChip({ mode, onPick }: ModeChipProps) {
  const [open, setOpen] = useState(false)
  const box = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return undefined
    const away = (event: PointerEvent) => !box.current?.contains(event.target as Node) && setOpen(false)
    const escape = (event: KeyboardEvent) => event.key === 'Escape' && setOpen(false)
    window.addEventListener('pointerdown', away)
    window.addEventListener('keydown', escape)
    return () => {
      window.removeEventListener('pointerdown', away)
      window.removeEventListener('keydown', escape)
    }
  }, [open])
  return (
    <div className={`modo ${mode}`} ref={box}>
      <button type="button" className="compositor-modo" aria-haspopup="menu" aria-expanded={open} title={`${t.modeHints[mode]} (Shift+Tab)`} onClick={() => setOpen(!open)}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d={ICONS[mode]} />
        </svg>
        {t.modeNames[mode]}
      </button>
      {open && (
        <div className="modo-menu" role="menu" aria-label={t.modeLabel}>
          {MODES.map((each) => (
            <button
              key={each}
              type="button"
              role="menuitemradio"
              aria-checked={each === mode}
              className={`${each} ${each === mode ? 'ativo' : ''}`}
              onClick={() => {
                onPick(each)
                setOpen(false)
              }}
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d={ICONS[each]} />
              </svg>
              <span>
                <b>{t.modeNames[each]}</b>
                <small>{t.modeHints[each]}</small>
              </span>
            </button>
          ))}
          <small className="modo-atalho">{t.modeShortcut}</small>
        </div>
      )}
    </div>
  )
}
