import { useEffect, useState } from 'react'
import { type Bot, type Memories, botMemories } from '../../api'
import { locale, t } from '../../i18n'

const day = (iso: string | null) => (iso ? new Date(iso).toLocaleDateString(locale, { day: 'numeric', month: 'short' }) : '')

/** "Memories: N saved ›" over the chat: what ai-memory keeps for this project, which the bot's CLI can read. */
export function MemoriesLine({ bot, openSignal = 0 }: { bot: Bot; openSignal?: number }) {
  const [memories, setMemories] = useState<Memories | null>(null)
  const [open, setOpen] = useState(false)
  useEffect(() => {
    if (openSignal > 0) setOpen(true)
  }, [openSignal])
  useEffect(() => {
    setMemories(null)
    setOpen(false)
    botMemories(bot.id).then(setMemories).catch(() => setMemories(null))
  }, [bot.id])
  const count = memories?.count ?? 0
  // Nothing to show takes no room, unless the person asked to see the memories.
  if (!memories?.known || (count === 0 && openSignal === 0)) return null
  return (
    <div className="memorias">
      <button type="button" className="memorias-linha" aria-expanded={open} disabled={count === 0} onClick={() => setOpen(!open)}>
        {count === 1 ? t.memoriesSavedOne : count > 0 ? t.memoriesSaved.replace('{n}', String(count)) : t.memoriesNone}
        {count > 0 && (
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d={open ? 'M6 15l6-6 6 6' : 'M9 6l6 6-6 6'} />
          </svg>
        )}
      </button>
      {open && (
        <div className="memorias-lista">
          <ul>
            {memories.pages.map((page) => (
              <li key={page.path}>
                <span className="memorias-tipo">{page.kind ?? ''}</span>
                <span className="memorias-titulo">{page.title ?? page.path}</span>
                <span className="memorias-data">{day(page.updated_at)}</span>
              </li>
            ))}
          </ul>
          {memories.web && (
            <a href={memories.web} target="_blank" rel="noreferrer">
              {t.openInMemory}
            </a>
          )}
        </div>
      )}
    </div>
  )
}
