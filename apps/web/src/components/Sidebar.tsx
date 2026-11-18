import { AGENTS, type AgentId } from '../agents'
import type { Bot } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import { nowStamp, t } from '../i18n'
import { Avatar } from './Avatar'
import { Icon } from './Icons'

export type Tab = 'xray' | 'efficiency' | 'agents' | 'settings'
export type RoomView = 'channel' | AgentId
export type ChatView = RoomView | 'bots' | `bot:${string}`

const ORDER: AgentId[] = ['raio', 'dieta', 'memo']

interface SidebarProps {
  tab: Tab
  chat: ChatView
  project: string
  typing: AgentId | null
  bots: Bot[]
  updatedAt: Date
  onTab: (tab: Tab) => void
  onChat: (view: ChatView) => void
}

/** The one navigation of the panel: sections, this project's channel and the agents' threads. */
export function Sidebar({ tab, chat, project, typing, bots, updatedAt, onTab, onChat }: SidebarProps) {
  const nav: { id: Tab; label: string; icon: 'xray' | 'efficiency' | 'agents' | 'sliders' }[] = [
    { id: 'xray', label: t.tabXray, icon: 'xray' },
    { id: 'efficiency', label: t.tabEfficiency, icon: 'efficiency' },
    { id: 'agents', label: t.tabAgents, icon: 'agents' },
    { id: 'settings', label: t.settingsTitle, icon: 'sliders' },
  ]
  const inChat = (view: ChatView) => tab === 'agents' && chat === view
  return (
    <aside className="lateral">
      <span className="marca">
        <Mark />
        raiox
      </span>
      <nav className="lado-grupo" aria-label="raiox">
        {nav.map((item) => (
          <button key={item.id} className={`lado-item ${tab !== item.id ? '' : item.id === 'agents' ? 'ativo-leve' : 'ativo'}`} aria-current={tab === item.id ? 'page' : undefined} onClick={() => onTab(item.id)}>
            <Icon name={item.icon} />
            <span className="nome">{item.label}</span>
          </button>
        ))}
      </nav>
      <div className="lado-grupo">
        <span className="lado-titulo">{t.channels}</span>
        <button className={`lado-item ${inChat('channel') ? 'ativo' : ''}`} onClick={() => onChat('channel')}>
          <Icon name="hash" />
          <span className="nome">{project}</span>
        </button>
      </div>
      <div className="lado-grupo">
        <span className="lado-titulo">{t.directs}</span>
        {ORDER.map((id) => (
          <button key={id} className={`lado-item ${inChat(id) ? 'ativo' : ''}`} onClick={() => onChat(id)}>
            <Avatar agent={AGENTS[id]} size={22} state={typing === id ? 'thinking' : 'idle'} />
            <span className="nome">{AGENTS[id].name}</span>
            <span className={`presenca ${typing === id ? 'digitando' : ''}`}>{typing === id ? t.typingShort : ''}</span>
          </button>
        ))}
      </div>
      <div className="lado-grupo">
        <span className="lado-titulo">{t.bots}</span>
        {bots.map((bot) => (
          <button key={bot.id} className={`lado-item ${inChat(`bot:${bot.id}`) ? 'ativo' : ''}`} onClick={() => onChat(`bot:${bot.id}`)}>
            <Avatar agent={faceForBot(bot)} size={22} state={bot.busy ? 'thinking' : 'idle'} />
            <span className="nome">{bot.name}</span>
            <span className={`presenca ${bot.busy ? 'digitando' : ''}`}>{bot.busy ? t.typingShort : (bot.persona?.name ?? AGENT_LABEL[bot.agent].split(' ')[0])}</span>
          </button>
        ))}
        <button className={`lado-item ${inChat('bots') ? 'ativo' : ''}`} onClick={() => onChat('bots')}>
          <Icon name="plus" />
          <span className="nome">{t.addBot}</span>
        </button>
      </div>
      <footer className="lado-rodape">
        <span>{nowStamp(updatedAt)}</span>
        <span className="estado">
          <i className="ponto ok" />
          {t.updated}
        </span>
        <span className="lado-nota">{t.roomRule}</span>
        <button className="lado-link" onClick={() => onTab('settings')}>
          {t.setupAgain}
        </button>
      </footer>
    </aside>
  )
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
