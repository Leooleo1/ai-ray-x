import { useEffect, useState } from 'react'
import type { Bot } from '../../api'
import { AGENT_LABEL, faceForBot } from '../../avatars'
import { t } from '../../i18n'
import { Avatar } from '../Avatar'

const SAVED = 'raiox.abas'
const MAIN = 'main'
/** Past this many, the oldest tab that is not open closes. */
const MAX_TABS = 10

interface Tab {
  bot: string
  thread: string
}

const same = (a: Tab, b: Tab) => a.bot === b.bot && a.thread === b.thread

function loadTabs(): Tab[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(SAVED) ?? '[]')
    if (!Array.isArray(parsed)) return []
    return parsed.filter((x): x is Tab => typeof x?.bot === 'string' && typeof x?.thread === 'string')
  } catch {
    return []
  }
}

function saveTabs(tabs: Tab[]) {
  try {
    localStorage.setItem(SAVED, JSON.stringify(tabs))
  } catch {
    // Private mode or a full storage: the tabs just last this visit.
  }
}

/** The tabs still open, without conversations that were closed or bots that are gone, the open one included. */
function withOpen(tabs: Tab[], open: Tab, bots: Bot[]): Tab[] {
  const alive = tabs.filter((tab) => {
    const bot = bots.find((b) => b.id === tab.bot)
    return bot && (tab.thread === MAIN || bot.threads.some((x) => x.id === tab.thread))
  })
  const all = alive.some((tab) => same(tab, open)) ? alive : [...alive, open]
  if (all.length <= MAX_TABS) return all
  const oldest = all.findIndex((tab) => !same(tab, open))
  return all.filter((_, i) => i !== oldest)
}

interface OpenTabsProps {
  bots: Bot[]
  open: Tab
  busy: (tab: Tab) => boolean
  onOpen: (tab: Tab) => void
  onNew: () => void
}

/** The conversations the person has open, as tabs across projects, and a new one in this project. */
export function OpenTabs({ bots, open, busy, onOpen, onNew }: OpenTabsProps) {
  const [tabs, setTabs] = useState<Tab[]>(() => withOpen(loadTabs(), open, bots))
  useEffect(() => {
    setTabs((was) => {
      const next = withOpen(was, open, bots)
      saveTabs(next)
      return next
    })
  }, [open.bot, open.thread, bots])
  const close = (tab: Tab) => {
    const at = tabs.findIndex((x) => same(x, tab))
    const next = tabs.filter((x) => !same(x, tab))
    setTabs(next)
    saveTabs(next)
    if (same(tab, open)) {
      const neighbour = next[Math.min(at, next.length - 1)]
      if (neighbour) onOpen(neighbour)
    }
  }
  return (
    <div className="abas" role="tablist" aria-label={t.openTabs}>
      {tabs.map((tab) => {
        const bot = bots.find((b) => b.id === tab.bot)
        if (!bot) return null
        const side = bot.threads.find((x) => x.id === tab.thread)
        const title = side ? side.title : bot.name
        const hint = [bot.name, side ? side.title : t.mainConversation, AGENT_LABEL[bot.agent]].join(' · ')
        const active = same(tab, open)
        return (
          <span key={`${tab.bot}:${tab.thread}`} className={`aba ${active ? 'ativa' : ''}`}>
            <button type="button" role="tab" aria-selected={active} title={hint} onClick={() => !active && onOpen(tab)}>
              <Avatar agent={faceForBot(bot)} size={18} state={busy(tab) ? 'thinking' : 'still'} />
              <b>{title}</b>
            </button>
            {tabs.length > 1 && (
              <button type="button" className="aba-fechar" aria-label={t.closeTab.replace('{c}', title)} title={t.closeTab.replace('{c}', title)} onClick={() => close(tab)}>
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M7 7l10 10M17 7L7 17" />
                </svg>
              </button>
            )}
          </span>
        )
      })}
      <button type="button" className="aba-nova" aria-label={t.newThread} title={t.newThread} onClick={onNew}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M12 5v14M5 12h14" />
        </svg>
      </button>
    </div>
  )
}
