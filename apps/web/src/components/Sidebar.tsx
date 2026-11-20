import { type CSSProperties, type FocusEvent, type MouseEvent, useEffect, useState } from 'react'
import type { Bot } from '../api'
import { faceForBot } from '../avatars'
import { t } from '../i18n'
import { Avatar } from './Avatar'
import { Icon, type IconName } from './Icons'
import { PlanRings } from './PlanRings'
import { ProfileMenu } from './ProfileMenu'
import './sidebar-nav.css'

export type Tab = 'xray' | 'usage' | 'efficiency' | 'flows' | 'agents' | 'desk' | 'deliveries' | 'wiki' | 'settings'
export type ChatView = 'bots' | 'grupo' | `bot:${string}` | `troca:${string}` | `grupo:${string}` | `canal:${string}`

/** The three pages about what the AIs consume, behind one item: the X-ray, Usage and Efficiency. */
export const CONSUMPTION: Tab[] = ['xray', 'usage', 'efficiency']
const LAST_CONSUMPTION = 'raiox.consumo'
/** The palette's keys as this computer writes them. */
const SHORTCUT = /Mac|iPhone|iPad/.test(navigator.platform) ? '⌘K' : 'Ctrl K'

/** The consumption page the person last looked at, so the item opens where they left it. */
export function lastConsumption(): Tab {
  try {
    const saved = localStorage.getItem(LAST_CONSUMPTION) as Tab | null
    return saved && CONSUMPTION.includes(saved) ? saved : 'xray'
  } catch {
    return 'xray'
  }
}

export function keepConsumption(tab: Tab) {
  try {
    if (CONSUMPTION.includes(tab)) localStorage.setItem(LAST_CONSUMPTION, tab)
  } catch {
    // Private mode: it opens on the X-ray next time.
  }
}

interface SidebarProps {
  tab: Tab
  chat: ChatView
  bots: Bot[]
  /** Bots with an answer not seen yet. */
  unread: Set<string>
  /** The channels, and whether a member works in each now. */
  channels: { id: string; name: string; working: boolean }[]
  updatedAt: Date
  collapsed: boolean
  onToggle: () => void
  onTab: (tab: Tab) => void
  onChat: (view: ChatView) => void
  /** Opens Settings on one of its tabs (from the profile menu). */
  onSettings: (tab: 'conexoes' | 'celular') => void
  onSetup: () => void
  /** Opens the ⌘K palette. */
  onSearch: () => void
}

/** The one navigation of the panel: the sections, then a row per project with a bot. */
export function Sidebar({ tab, chat, bots, unread, channels, updatedAt, collapsed, onToggle, onTab, onChat, onSettings, onSetup, onSearch }: SidebarProps) {
  const [tip, setTip] = useState<{ text: string; top: number; left: number } | null>(null)
  useEffect(() => {
    if (!collapsed) setTip(null)
  }, [collapsed])
  const showTip = (text: string) => (event: MouseEvent<HTMLElement> | FocusEvent<HTMLElement>) => {
    if (!collapsed) return
    const rect = event.currentTarget.getBoundingClientRect()
    setTip({ text, top: rect.top + rect.height / 2, left: rect.right + 10 })
  }
  const hideTip = () => setTip(null)
  const tipProps = (text: string) => ({
    onMouseEnter: showTip(text),
    onMouseLeave: hideTip,
    onFocus: showTip(text),
    onBlur: hideTip,
  })
  const [adding, setAdding] = useState(false)
  const [more, setMore] = useState(false)
  // The + menu and the phone's More close on a click elsewhere or Escape.
  useEffect(() => {
    if (!adding && !more) return undefined
    const away = (event: PointerEvent) => {
      if (!(event.target as Element).closest('.lado-novo, .lado-novo-menu, .lado-mais, .lado-mais-folha')) {
        setAdding(false)
        setMore(false)
      }
    }
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      setAdding(false)
      setMore(false)
    }
    window.addEventListener('pointerdown', away)
    window.addEventListener('keydown', escape)
    return () => {
      window.removeEventListener('pointerdown', away)
      window.removeEventListener('keydown', escape)
    }
  }, [adding, more])
  // The work first, then one item for consumption. On a phone the last three move under More.
  const nav: { id: Tab; label: string; short?: string; icon: IconName; underMore?: boolean; phoneOnly?: boolean }[] = [
    { id: 'agents', label: t.tabAgents, short: t.agentsShort, icon: 'agents' },
    { id: 'desk', label: t.deskTab, icon: 'desk' },
    { id: 'flows', label: t.flowsTab, icon: 'flows' },
    { id: 'deliveries', label: t.deliveriesTab, icon: 'deliveries', underMore: true },
    { id: 'wiki', label: t.wikiTab, icon: 'wiki', underMore: true },
    { id: lastConsumption(), label: t.consumptionTab, icon: 'xray' },
    { id: 'settings', label: t.settingsTitle, short: t.settingsShort, icon: 'sliders', underMore: true, phoneOnly: true },
  ]
  const isOn = (id: Tab) => tab === id || (CONSUMPTION.includes(id) && CONSUMPTION.includes(tab))
  const go = (id: Tab) => {
    setMore(false)
    onTab(id)
  }
  const toggleLabel = collapsed ? t.sidebarExpand : t.sidebarCollapse
  return (
    <aside className={`lateral ${collapsed ? 'recolhida' : ''}`}>
      <div className="lado-topo" data-tauri-drag-region>
        <span className="marca" title="raiox" data-tauri-drag-region>
          <Mark />
          <span className="marca-nome">raiox</span>
        </span>
        <button
          type="button"
          className="lado-recolher"
          onClick={() => {
            hideTip()
            onToggle()
          }}
          aria-label={toggleLabel}
          aria-expanded={!collapsed}
          title={collapsed ? undefined : toggleLabel}
          {...tipProps(toggleLabel)}
        >
          <Icon name={collapsed ? 'sidebar-expand' : 'sidebar-collapse'} />
        </button>
      </div>
      <button type="button" className="lado-busca" aria-label={collapsed ? t.paletteLabel : undefined} onClick={onSearch} {...tipProps(`${t.paletteSearch} ${SHORTCUT}`)}>
        <Icon name="search" />
        <span className="nome">{t.paletteSearch}</span>
        <kbd>{SHORTCUT}</kbd>
      </button>
      <div className="lado-scroll" onScroll={hideTip}>
        <nav className="lado-grupo lado-menu" aria-label="raiox">
          {nav.map((item) => (
            <button
              key={item.label}
              type="button"
              className={`lado-item ${item.phoneOnly ? 'so-celular' : ''} ${item.underMore ? 'no-mais' : ''} ${CONSUMPTION.includes(item.id) ? 'lado-consumo' : ''} ${!isOn(item.id) ? '' : item.id === 'agents' ? 'ativo-leve' : 'ativo'}`}
              aria-current={isOn(item.id) ? 'page' : undefined}
              aria-label={collapsed ? item.label : undefined}
              onClick={() => go(item.id)}
              {...tipProps(item.label)}
            >
              <Icon name={item.icon} />
              <span className="nome">{item.label}</span>
              {item.short && <span className="nome curto">{item.short}</span>}
            </button>
          ))}
          <button
            type="button"
            className={`lado-item so-celular lado-mais ${nav.some((item) => item.underMore && isOn(item.id)) ? 'ativo' : ''}`}
            aria-expanded={more}
            onClick={() => setMore(!more)}
          >
            <Icon name="more" />
            <span className="nome">{t.moreTab}</span>
          </button>
          {more && (
            <div className="lado-mais-folha" role="menu">
              {nav
                .filter((item) => item.underMore)
                .map((item) => (
                  <button key={item.label} type="button" role="menuitem" className={isOn(item.id) ? 'ativo' : ''} onClick={() => go(item.id)}>
                    <Icon name={item.icon} />
                    {item.label}
                  </button>
                ))}
            </div>
          )}
        </nav>
        <div className="lado-grupo lado-projetos">
          <div className="lado-titulo lado-titulo-acao">
            <span>{t.projects}</span>
            <button type="button" className="lado-novo" aria-label={t.newSomething} title={t.newSomething} aria-expanded={adding} onClick={() => setAdding(!adding)}>
              <Icon name="plus" />
            </button>
            {adding && (
              <div className="lado-novo-menu" role="menu">
                {[
                  { view: 'bots' as ChatView, icon: 'plus' as IconName, label: t.addProject },
                  { view: 'canal:novo' as ChatView, icon: 'hash' as IconName, label: t.channelNewNav },
                  { view: 'grupo' as ChatView, icon: 'ask-many' as IconName, label: t.askMany },
                ].map((entry) => (
                  <button
                    key={entry.view}
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      setAdding(false)
                      onChat(entry.view)
                    }}
                  >
                    <Icon name={entry.icon} />
                    {entry.label}
                  </button>
                ))}
              </div>
            )}
          </div>
          {/* One row per project; a project can have bots on several CLIs, the newest one opens. */}
          {projectsOf(bots).map(({ name, latest, busy, agents }) => {
            const hasUnread = bots.some((b) => b.name === name && unread.has(b.id))
            const hint = `${name} · ${busy ? t.typingShort : agents}`
            return (
              <button
                key={name}
                type="button"
                className={`lado-item ${tab === 'agents' && bots.some((b) => b.name === name && chat === `bot:${b.id}`) ? 'ativo' : ''}`}
                aria-label={collapsed ? hint : undefined}
                onClick={() => onChat(`bot:${latest.id}`)}
                {...tipProps(hint)}
              >
                <span className="lado-rosto">
                  <Avatar agent={faceForBot(latest)} size={22} state={busy ? 'thinking' : 'idle'} />
                  {collapsed && (hasUnread || busy) && <i className={`lado-sinal ${hasUnread ? 'nao-lida' : 'digitando'}`} aria-hidden="true" />}
                </span>
                <span className="nome">{name}</span>
                {hasUnread ? (
                  <i className="nao-lida" aria-label={t.unreadAnswer} />
                ) : (
                  <span className={`presenca ${busy ? 'digitando' : ''}`}>{busy ? t.typingShort : agents}</span>
                )}
              </button>
            )
          })}
          {channels.length > 0 && <span className="lado-titulo lado-titulo-canais">{t.channels}</span>}
          {channels.map((channel) => {
            const hint = `# ${channel.name}${channel.working ? ` · ${t.typingShort}` : ''}`
            return (
              <button
                key={channel.id}
                type="button"
                className={`lado-item ${tab === 'agents' && chat === `canal:${channel.id}` ? 'ativo' : ''}`}
                aria-label={collapsed ? hint : undefined}
                onClick={() => onChat(`canal:${channel.id}`)}
                {...tipProps(hint)}
              >
                <span className="lado-rosto">
                  <Icon name="hash" />
                  {collapsed && channel.working && <i className="lado-sinal digitando" aria-hidden="true" />}
                </span>
                <span className="nome">{channel.name}</span>
                {channel.working && <span className="presenca digitando">{t.typingShort}</span>}
              </button>
            )
          })}
        </div>
      </div>
      <footer className="lado-rodape">
        <PlanRings collapsed={collapsed} />
        <ProfileMenu bots={bots} updatedAt={updatedAt} collapsed={collapsed} onSettings={onSettings} onSetup={onSetup} />
      </footer>
      {collapsed && tip && (
        <span className="lado-dica" role="tooltip" style={{ top: tip.top, left: tip.left }}>
          {tip.text}
        </span>
      )}
    </aside>
  )
}

const SHORT: Record<string, string> = { claude: 'Claude', codex: 'Codex', cursor: 'Cursor', antigravity: 'Antigravity', opencode: 'opencode' }

/** The bots grouped by project, newest activity first. */
function projectsOf(bots: Bot[]) {
  const lastMs = (b: Bot) => b.messages.at(-1)?.ms ?? 0
  const names = [...new Set([...bots].sort((a, b) => lastMs(b) - lastMs(a)).map((b) => b.name))]
  return names.map((name) => {
    const own = bots.filter((b) => b.name === name).sort((a, b) => lastMs(b) - lastMs(a))
    return { name, latest: own[0], busy: own.some((b) => b.busy), agents: `${SHORT[own[0].agent] ?? own[0].agent}${own.length > 1 ? ` +${own.length - 1}` : ''}` }
  })
}

export function Mark() {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true">
      {[
        [16, 16, 0],
        [12, 12, 1],
        [20, 12, 1],
        [12, 20, 1],
        [20, 20, 1],
        [8, 8, 2],
        [24, 8, 2],
        [8, 24, 2],
        [24, 24, 2],
      ].map(([cx, cy, ring]) => (
        <circle key={`${cx}-${cy}`} cx={cx} cy={cy} r={2.4} style={{ '--r': ring } as CSSProperties} />
      ))}
    </svg>
  )
}
