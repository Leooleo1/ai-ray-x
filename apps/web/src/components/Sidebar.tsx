import type { Bot } from '../api'
import { faceForBot } from '../avatars'
import { t } from '../i18n'
import { Avatar } from './Avatar'
import { Icon } from './Icons'
import { ProfileMenu } from './ProfileMenu'
import { UpdateNote } from './UpdateNote'

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
  onTab: (tab: Tab) => void
  onChat: (view: ChatView) => void
  /** Opens Settings on one of its tabs (from the profile menu). */
  onSettings: (tab: 'conexoes' | 'celular') => void
  onSetup: () => void
}

/** The one navigation of the panel: the sections, then a row per project with a bot. */
export function Sidebar({ tab, chat, bots, unread, channels, updatedAt, onTab, onChat, onSettings, onSetup }: SidebarProps) {
  const nav: { id: Tab; label: string; short?: string; icon: 'xray' | 'efficiency' | 'agents' | 'desk' | 'deliveries' | 'wiki' | 'sliders' }[] = [
    { id: 'xray', label: t.tabXray, icon: 'xray' },
    { id: 'efficiency', label: t.tabEfficiency, short: t.efficiencyShort, icon: 'efficiency' },
    { id: 'agents', label: t.tabAgents, short: t.agentsShort, icon: 'agents' },
    { id: 'desk', label: t.deskTab, icon: 'desk' },
    { id: 'deliveries', label: t.deliveriesTab, icon: 'deliveries' },
    { id: 'wiki', label: t.wikiTab, icon: 'wiki' },
    { id: 'settings', label: t.settingsTitle, short: t.settingsShort, icon: 'sliders' },
  ]
  return (
    <aside className="lateral">
      <span className="marca">
        <Mark />
        raiox
      </span>
      <nav className="lado-grupo lado-menu" aria-label="raiox">
        {nav.map((item) => (
          <button key={item.id} className={`lado-item ${item.id === 'settings' ? 'so-celular' : ''} ${tab !== item.id ? '' : item.id === 'agents' ? 'ativo-leve' : 'ativo'}`} aria-current={tab === item.id ? 'page' : undefined} onClick={() => onTab(item.id)}>
            <Icon name={item.icon} />
            <span className="nome">{item.label}</span>
            {item.short && <span className="nome curto">{item.short}</span>}
          </button>
        ))}
      </nav>
      <div className="lado-grupo lado-projetos">
        <span className="lado-titulo">{t.projects}</span>
        {/* One row per project; a project can have bots on several CLIs, the newest one opens. */}
        {projectsOf(bots).map(({ name, latest, busy, agents }) => (
          <button key={name} className={`lado-item ${tab === 'agents' && bots.some((b) => b.name === name && chat === `bot:${b.id}`) ? 'ativo' : ''}`} onClick={() => onChat(`bot:${latest.id}`)}>
            <Avatar agent={faceForBot(latest)} size={22} state={busy ? 'thinking' : 'idle'} />
            <span className="nome">{name}</span>
            {bots.some((b) => b.name === name && unread.has(b.id)) ? (
              <i className="nao-lida" aria-label={t.unreadAnswer} />
            ) : (
              <span className={`presenca ${busy ? 'digitando' : ''}`}>{busy ? t.typingShort : agents}</span>
            )}
          </button>
        ))}
        {channels.length > 0 && <span className="lado-titulo lado-titulo-canais">{t.channels}</span>}
        {channels.map((channel) => (
          <button key={channel.id} className={`lado-item ${tab === 'agents' && chat === `canal:${channel.id}` ? 'ativo' : ''}`} onClick={() => onChat(`canal:${channel.id}`)}>
            <span className="lado-cerquilha" aria-hidden="true">
              #
            </span>
            <span className="nome">{channel.name}</span>
            {channel.working && <span className="presenca digitando">{t.typingShort}</span>}
          </button>
        ))}
        <button className={`lado-item ${tab === 'agents' && chat === 'canal:novo' ? 'ativo' : ''}`} onClick={() => onChat('canal:novo')}>
          <span className="lado-cerquilha" aria-hidden="true">
            #
          </span>
          <span className="nome">{t.channelNewNav}</span>
        </button>
        <button className={`lado-item ${tab === 'agents' && chat.startsWith('grupo') ? 'ativo' : ''}`} onClick={() => onChat('grupo')}>
          <Icon name="ask-many" />
          <span className="nome">{t.askMany}</span>
        </button>
        <button className={`lado-item ${tab === 'agents' && chat === 'bots' ? 'ativo' : ''}`} onClick={() => onChat('bots')}>
          <Icon name="plus" />
          <span className="nome">{t.addProject}</span>
        </button>
      </div>
      <footer className="lado-rodape">
        <UpdateNote />
        <ProfileMenu bots={bots} updatedAt={updatedAt} onSettings={onSettings} onSetup={onSetup} />
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
        [8, 8], [24, 8], [12, 12], [20, 12], [16, 16], [12, 20], [20, 20], [8, 24], [24, 24],
      ].map(([cx, cy]) => (
        <circle key={`${cx}-${cy}`} cx={cx} cy={cy} r={2.4} />
      ))}
    </svg>
  )
}
