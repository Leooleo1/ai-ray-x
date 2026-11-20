import type { CSSProperties } from 'react'
import type { Bot } from '../api'
import { faceForBot } from '../avatars'
import { t } from '../i18n'
import { Avatar } from './Avatar'
import { Icon, type IconName } from './Icons'
import { ProfileMenu } from './ProfileMenu'

export type Tab = 'xray' | 'efficiency' | 'agents' | 'desk' | 'deliveries' | 'wiki' | 'settings'
export type ChatView = 'bots' | 'grupo' | `bot:${string}` | `troca:${string}` | `grupo:${string}` | `canal:${string}`

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
}

/** The one navigation of the panel: the sections, then a row per project with a bot. */
export function Sidebar({ tab, chat, bots, unread, channels, updatedAt, collapsed, onToggle, onTab, onChat, onSettings, onSetup }: SidebarProps) {
  const nav: { id: Tab; label: string; short?: string; icon: IconName }[] = [
    { id: 'xray', label: t.tabXray, icon: 'xray' },
    { id: 'efficiency', label: t.tabEfficiency, short: t.efficiencyShort, icon: 'efficiency' },
    { id: 'agents', label: t.tabAgents, short: t.agentsShort, icon: 'agents' },
    { id: 'desk', label: t.deskTab, icon: 'desk' },
    { id: 'deliveries', label: t.deliveriesTab, icon: 'deliveries' },
    { id: 'wiki', label: t.wikiTab, icon: 'wiki' },
    { id: 'settings', label: t.settingsTitle, short: t.settingsShort, icon: 'sliders' },
  ]
  const toggleLabel = collapsed ? t.sidebarExpand : t.sidebarCollapse
  return (
    <aside className={`lateral ${collapsed ? 'recolhida' : ''}`}>
      <div className="lado-topo">
        <span className="marca" title="raiox">
          <Mark />
          <span className="marca-nome">raiox</span>
        </span>
        <button
          type="button"
          className="lado-recolher"
          onClick={onToggle}
          aria-label={toggleLabel}
          aria-expanded={!collapsed}
          title={toggleLabel}
        >
          <Icon name={collapsed ? 'sidebar-expand' : 'sidebar-collapse'} />
        </button>
      </div>
      <div className="lado-scroll">
        <nav className="lado-grupo lado-menu" aria-label="raiox">
          {nav.map((item) => (
            <button
              key={item.id}
              className={`lado-item ${item.id === 'settings' ? 'so-celular' : ''} ${tab !== item.id ? '' : item.id === 'agents' ? 'ativo-leve' : 'ativo'}`}
              aria-current={tab === item.id ? 'page' : undefined}
              aria-label={collapsed ? item.label : undefined}
              title={collapsed ? item.label : undefined}
              onClick={() => onTab(item.id)}
            >
              <Icon name={item.icon} />
              <span className="nome">{item.label}</span>
              {item.short && <span className="nome curto">{item.short}</span>}
            </button>
          ))}
        </nav>
        <div className="lado-grupo lado-projetos">
          <span className="lado-titulo">{t.projects}</span>
          {/* One row per project; a project can have bots on several CLIs, the newest one opens. */}
          {projectsOf(bots).map(({ name, latest, busy, agents }) => {
            const hasUnread = bots.some((b) => b.name === name && unread.has(b.id))
            const hint = `${name} · ${busy ? t.typingShort : agents}`
            return (
              <button
                key={name}
                className={`lado-item ${tab === 'agents' && bots.some((b) => b.name === name && chat === `bot:${b.id}`) ? 'ativo' : ''}`}
                aria-label={collapsed ? hint : undefined}
                title={collapsed ? hint : undefined}
                onClick={() => onChat(`bot:${latest.id}`)}
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
                className={`lado-item ${tab === 'agents' && chat === `canal:${channel.id}` ? 'ativo' : ''}`}
                aria-label={collapsed ? hint : undefined}
                title={collapsed ? hint : undefined}
                onClick={() => onChat(`canal:${channel.id}`)}
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
          <button
            className={`lado-item ${tab === 'agents' && chat === 'canal:novo' ? 'ativo' : ''}`}
            aria-label={collapsed ? t.channelNewNav : undefined}
            title={collapsed ? t.channelNewNav : undefined}
            onClick={() => onChat('canal:novo')}
          >
            <Icon name="hash" />
            <span className="nome">{t.channelNewNav}</span>
          </button>
          <button
            className={`lado-item ${tab === 'agents' && chat.startsWith('grupo') ? 'ativo' : ''}`}
            aria-label={collapsed ? t.askMany : undefined}
            title={collapsed ? t.askMany : undefined}
            onClick={() => onChat('grupo')}
          >
            <Icon name="ask-many" />
            <span className="nome">{t.askMany}</span>
          </button>
          <button
            className={`lado-item ${tab === 'agents' && chat === 'bots' ? 'ativo' : ''}`}
            aria-label={collapsed ? t.addProject : undefined}
            title={collapsed ? t.addProject : undefined}
            onClick={() => onChat('bots')}
          >
            <Icon name="plus" />
            <span className="nome">{t.addProject}</span>
          </button>
        </div>
      </div>
      <footer className="lado-rodape">
        <ProfileMenu bots={bots} updatedAt={updatedAt} collapsed={collapsed} onSettings={onSettings} onSetup={onSetup} />
      </footer>
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
