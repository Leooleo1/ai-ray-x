import { useEffect, useState } from 'react'
import { type Bot, type Memories, botMemories } from '../../api'
import { locale, t } from '../../i18n'

const day = (iso: string | null) => (iso ? new Date(iso).toLocaleDateString(locale, { day: 'numeric', month: 'short' }) : '')

/** "Memories: N saved ›" over the chat: what ai-memory keeps for this project, which the bot's CLI can read. */
/** `version` changes when an answer is kept, so the count follows. */
export function MemoriesLine({ bot, openSignal = 0, version = 0 }: { bot: Bot; openSignal?: number; version?: number }) {
  const [memories, setMemories] = useState<Memories | null>(null)
  const [open, setOpen] = useState(false)
  useEffect(() => {
    if (openSignal > 0) setOpen(true)
  }, [openSignal])
  useEffect(() => {
    setMemories(null)
    setOpen(false)
  }, [bot.id])
  useEffect(() => {
    botMemories(bot.id).then(setMemories).catch(() => setMemories(null))
  }, [bot.id, version])
  const count = memories?.count ?? 0
  // Nothing to show takes no room, unless the person asked to see the memories.
  if (!memories?.known || (count === 0 && openSignal === 0)) return null
  return (
    <div className="memorias">
      <button
        type="button"
        className={`icone-btn topo-contador ${open ? 'ativo' : ''}`}
        aria-expanded={open}
        disabled={count === 0}
        aria-label={count === 1 ? t.memoriesSavedOne : count > 0 ? t.memoriesSaved.replace('{n}', String(count)) : t.memoriesNone}
        title={count === 1 ? t.memoriesSavedOne : count > 0 ? t.memoriesSaved.replace('{n}', String(count)) : t.memoriesNone}
        onClick={() => setOpen(!open)}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M6 4.5h9.5A2.5 2.5 0 0 1 18 7v12.5H8.5A2.5 2.5 0 0 1 6 17z" />
          <path d="M6 17a2.5 2.5 0 0 1 2.5-2.5H18" />
          <path d="M10 8.5h4" />
        </svg>
        {count > 0 && <small>{count}</small>}
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
