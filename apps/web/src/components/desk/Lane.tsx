import { Skeleton } from '../Skeleton'
import type { ReactNode } from 'react'
import type { TaskStatus } from '../../api'

/** Whose move it is: the person's (ember) or an AI's (cobalt). The lane lights up only with something in it. */
export type LaneTone = 'brasa' | 'cobalto'

interface LaneProps {
  /** Where the lane sits in a task's life; small screens put the person's lane first. */
  status: TaskStatus
  title: string
  count: number
  tone?: LaneTone
  /** What an empty lane says: what lands there, or how to fill it. */
  empty: string
  /** A picture over that sentence, for the lane that sits empty most of the time. */
  emptyArt?: string
  /** The lane's items are still on their way: its shape shows instead. */
  loading?: boolean
  children?: ReactNode
}

/** One lane of the desk: its name, how many things are in it, and the things. */
export function Lane({ status, title, count, tone, empty, emptyArt, loading = false, children }: LaneProps) {
  const lit = tone && count > 0 ? `acesa ${tone}` : ''
  return (
    <section className={`mesa-raia ${status} ${lit}`} aria-label={`${title}: ${count}`}>
      <header className="mesa-raia-topo">
        <h3>{title}</h3>
        <span className="mesa-raia-conta" aria-hidden="true">
          {count}
        </span>
      </header>
      {loading && count === 0 ? (
        <Skeleton rows={3} />
      ) : count === 0 ? (
        <div className="mesa-raia-vazia">
          {emptyArt && <img className="arte-vazia pelucia" src={emptyArt} alt="" />}
          <p>{empty}</p>
        </div>
      ) : (
        <div className="mesa-raia-itens">{children}</div>
      )}
    </section>
  )
}
