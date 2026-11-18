import { useEffect, useState } from 'react'
import { compact, t } from '../i18n'

export interface BarRow {
  label: string
  value: number
  /** Overrides the formatted value on the right. */
  display?: string
}

interface BarsProps {
  title: string
  subtitle?: string
  rows: BarRow[]
  unit?: string
  /** The bar's color; the fill fades from a light tint of it. */
  color?: string
}

const DEFAULT_COLOR = '#6f95f2'

/** Horizontal bars with a pixel-dithered fill, switchable to a plain table. */
export function Bars({ title, subtitle, rows, unit = '', color = DEFAULT_COLOR }: BarsProps) {
  const [asTable, setAsTable] = useState(false)
  const [grown, setGrown] = useState(false)
  useEffect(() => {
    const frame = window.requestAnimationFrame(() => setGrown(true))
    return () => window.cancelAnimationFrame(frame)
  }, [])
  const max = Math.max(...rows.map((r) => r.value), 0)
  const show = (row: BarRow) => row.display ?? `${compact(row.value)}${unit ? ` ${unit}` : ''}`
  return (
    <figure className="barras" style={{ ['--cor' as string]: color }}>
      <figcaption>
        <b>{title}</b>
        <button type="button" className="barras-modo" onClick={() => setAsTable(!asTable)}>
          {asTable ? t.asBars : t.asTable}
        </button>
        {subtitle && <small>{subtitle}</small>}
      </figcaption>
      {asTable ? (
        <table>
          <tbody>
            {rows.map((row) => (
              <tr key={row.label}>
                <th scope="row">{row.label}</th>
                <td>{show(row)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <ul>
          {rows.map((row) => (
            <li key={row.label}>
              <span className="barras-rotulo">{row.label}</span>
              <span className="barras-trilho">
                <i style={{ width: grown && max > 0 ? `max(${(row.value / max) * 100}%, 14px)` : 0 }} />
              </span>
              <span className="barras-valor">{show(row)}</span>
            </li>
          ))}
        </ul>
      )}
    </figure>
  )
}
