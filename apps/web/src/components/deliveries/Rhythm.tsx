import type { Delivery } from '../../api'
import { t } from '../../i18n'
import { RHYTHM_DAYS, WEEK_DAYS, dayLabel, dayStart, lastDays, toneOf } from './days'

/** Each day is a small field of pixels: this many across and up; one lit pixel per delivery. */
const COLS = 3
const ROWS = 6
const SLOTS = COLS * ROWS

interface RhythmProps {
  items: Delivery[]
  /** The day the list shows alone, by its local midnight. */
  selected: number | null
  onPick: (day: number | null) => void
}

/** The last two weeks as a dot-matrix: what the AIs delivered each day, by colour; a day opens on a click. */
export function Rhythm({ items, selected, onPick }: RhythmProps) {
  const days = lastDays(RHYTHM_DAYS)
  const today = days[days.length - 1]
  const weekStart = days[days.length - WEEK_DAYS]
  const week = items.filter((item) => item.ms >= weekStart).length
  return (
    <section className="entregas-ritmo" aria-label={t.deliveriesRhythm}>
      <header>
        <h2>{t.deliveriesRhythm}</h2>
        <small>{t.deliveriesWeek.replace('{n}', String(week))}</small>
        <ul className="entregas-legenda" aria-hidden="true">
          <li className="codigo">{t.deliveriesToneCode}</li>
          <li className="feita">{t.deliveriesToneDone}</li>
          <li className="outra">{t.deliveriesToneOther}</li>
        </ul>
      </header>
      <div className="entregas-ritmo-dias">
        {days.map((start) => {
          // Bottom-up, oldest of the day first, like a pile.
          const dayItems = items.filter((item) => dayStart(item.ms) === start).reverse()
          const isPicked = selected === start
          return (
            <button
              key={start}
              type="button"
              className={`${isPicked ? 'escolhido' : ''} ${start === today ? 'hoje' : ''}`}
              aria-pressed={isPicked}
              aria-label={`${dayLabel(start)}: ${dayItems.length}`}
              disabled={dayItems.length === 0}
              onClick={() => onPick(isPicked ? null : start)}
            >
              <span className="entregas-campo">
                {Array.from({ length: SLOTS }, (_, slot) => {
                  const fromBottom = (ROWS - 1 - Math.floor(slot / COLS)) * COLS + (slot % COLS)
                  const item = dayItems[fromBottom]
                  return <i key={slot} className={item ? toneOf(item) : ''} />
                })}
              </span>
              <small>{new Date(start).getDate()}</small>
            </button>
          )
        })}
      </div>
    </section>
  )
}
