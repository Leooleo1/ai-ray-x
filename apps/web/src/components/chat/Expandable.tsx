import { useState } from 'react'
import { t } from '../../i18n'
import { Markdown } from '../Markdown'

/** Past this length, only the first paragraph shows until the reader opens the rest. */
const FOLD_AT = 280

/** A long message from another agent: its first paragraph and "Show more", which opens the rest in place, for good. */
export function Expandable({ text }: { text: string }) {
  const [open, setOpen] = useState(false)
  const first = text.split(/\n\s*\n/)[0] ?? text
  const folds = !open && text.length > FOLD_AT
  const shown = folds ? (first.length > FOLD_AT ? `${first.slice(0, FOLD_AT).trimEnd()}…` : first) : text
  return (
    <>
      <Markdown text={shown} />
      {folds && (
        <button type="button" className="ver-mais" onClick={() => setOpen(true)}>
          {t.showMore}
        </button>
      )}
    </>
  )
}
