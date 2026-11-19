import type { Bot } from '../api'
import { faceForBot } from '../avatars'
import { nowStamp, t } from '../i18n'
import { Avatar } from './Avatar'
import { Icon } from './Icons'

export type Tab = 'xray' | 'efficiency' | 'agents' | 'desk' | 'settings'
export type ChatView = 'bots' | 'grupo' | `bot:${string}` | `troca:${string}` | `grupo:${string}`

interface SidebarProps {
  tab: Tab
  chat: ChatView
  bots: Bot[]
  /** Bots with an answer not seen yet. */
  unread: Set<string>
  updatedAt: Date
  onTab: (tab: Tab) => void
  onChat: (view: ChatView) => void
}

/** The one navigation of the panel: the sections, then a row per project with a bot. */
export function Sidebar({ tab, chat, bots, unread, updatedAt, onTab, onChat }: SidebarProps) {
  const nav: { id: Tab; label: string; short?: string; icon: 'xray' | 'efficiency' | 'agents' | 'desk' | 'sliders' }[] = [
    { id: 'xray', label: t.tabXray, icon: 'xray' },
    { id: 'efficiency', label: t.tabEfficiency, icon: 'efficiency' },
    { id: 'agents', label: t.tabAgents, icon: 'agents' },
    { id: 'desk', label: t.deskTab, icon: 'desk' },
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
          <button key={item.id} className={`lado-item ${tab !== item.id ? '' : item.id === 'agents' ? 'ativo-leve' : 'ativo'}`} aria-current={tab === item.id ? 'page' : undefined} onClick={() => onTab(item.id)}>
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
        <span>{nowStamp(updatedAt)}</span>
        <span className="estado">
          <i className="ponto ok" />
          {t.updated}
        </span>
        <span className="lado-nota">{t.roomRule}</span>
      </footer>
    </aside>
  )
}

const SHORT: Record<string, string> = { claude: 'Claude', codex: 'Codex', cursor: 'Cursor' }

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
