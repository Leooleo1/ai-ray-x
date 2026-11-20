import { useLayoutEffect, useRef, useState } from 'react'

/** One glass track with a thumb that slides to the active option. */
interface Option<T extends string> {
  id: T
  label: string
}

interface SegmentedProps<T extends string> {
  options: Option<T>[]
  value: T
  onChange: (id: T) => void
  label: string
}

interface Thumb {
  left: number
  width: number
}

export function Segmented<T extends string>({ options, value, onChange, label }: SegmentedProps<T>) {
  const track = useRef<HTMLDivElement>(null)
  const [thumb, setThumb] = useState<Thumb | null>(null)
  useLayoutEffect(() => {
    const el = track.current
    if (!el) return undefined
    const measure = () => {
      const active = el.querySelector<HTMLButtonElement>(`[data-id="${value}"]`)
      if (active) setThumb({ left: active.offsetLeft, width: active.offsetWidth })
    }
    measure()
    if (typeof ResizeObserver === 'undefined') return undefined
    const observer = new ResizeObserver(measure)
    observer.observe(el)
    return () => observer.disconnect()
  }, [value, options.length])
  return (
    <div className="segmentado" role="tablist" aria-label={label} ref={track}>
      {thumb && <span className="trilho-ativo" style={{ transform: `translateX(${thumb.left}px)`, width: thumb.width }} />}
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          data-id={option.id}
          role="tab"
          aria-selected={option.id === value}
          className={option.id === value ? 'ativo' : ''}
          onClick={() => onChange(option.id)}
        >
          {option.label}
        </button>
      ))}
    </div>
  )
}
