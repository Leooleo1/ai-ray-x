// A short note that something the person did took effect: saved, approved, applied, undone.

export type NoticeKind = 'ok' | 'undo'

export interface Notice {
  id: number
  text: string
  kind: NoticeKind
}

const EVENT = 'raiox:aviso'
let next = 0

/** Shows a one-line note at the foot of the panel for a moment. */
export function notify(text: string, kind: NoticeKind = 'ok'): void {
  next += 1
  window.dispatchEvent(new CustomEvent<Notice>(EVENT, { detail: { id: next, text, kind } }))
}

/** Calls `on` with every note shown from now on; returns how to stop. */
export function onNotice(on: (notice: Notice) => void): () => void {
  const listener = (event: Event) => on((event as CustomEvent<Notice>).detail)
  window.addEventListener(EVENT, listener)
  return () => window.removeEventListener(EVENT, listener)
}
