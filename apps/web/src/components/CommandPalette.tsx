import { type ReactNode, useEffect, useMemo, useRef, useState } from 'react'
import { type Bot, type Flow, fetchFlows, projectBots } from '../api'
import { AGENT_LABEL, faceForBot } from '../avatars'
import { t } from '../i18n'
import { Avatar } from './Avatar'
import { Icon, type IconName } from './Icons'
import type { ChatView, Tab } from './Sidebar'
import './command-palette.css'

/** Most results shown at once; the person narrows by typing. */
const MAX_RESULTS = 40

interface Item {
  key: string
  group: string
  label: string
  hint?: string
  icon: ReactNode
  run: () => void
}

/** Lower case, no accents: "Configurações" matches "configuracoes". */
const plain = (text: string) =>
  text
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()

/** Every word typed appears somewhere in the label or the hint. */
const matches = (item: Item, words: string[]) => {
  const haystack = plain(`${item.label} ${item.hint ?? ''} ${item.group}`)
  return words.every((word) => haystack.includes(word))
}

interface CommandPaletteProps {
  bots: Bot[]
  channels: { id: string; name: string }[]
  onClose: () => void
  onGo: (tab: Tab) => void
  onChat: (view: ChatView) => void
  onThread: (bot: string, thread: string) => void
  onFlow: (id: string) => void
}

/** ⌘K: one field to reach any page, project, conversation, workflow or action. */
export function CommandPalette({ bots, channels, onClose, onGo, onChat, onThread, onFlow }: CommandPaletteProps) {
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const [flows, setFlows] = useState<Flow[]>([])
  const list = useRef<HTMLUListElement>(null)
  useEffect(() => {
    fetchFlows()
      .then(({ flows: found }) => setFlows(found))
      .catch(() => undefined)
  }, [])
  const items = useMemo<Item[]>(() => {
    const page = (tab: Tab, label: string, icon: IconName): Item => ({ key: `p-${tab}`, group: t.paletteGo, label, icon: <Icon name={icon} />, run: () => onGo(tab) })
    const action = (key: string, label: string, icon: IconName, run: () => void): Item => ({ key: `a-${key}`, group: t.paletteActions, label, icon: <Icon name={icon} />, run })
    const realBots = projectBots(bots)
    const projects = [...new Map(realBots.map((bot) => [bot.name, bot])).values()]
    return [
      page('agents', t.tabAgents, 'agents'),
      page('desk', t.deskTab, 'desk'),
      page('flows', t.flowsTab, 'flows'),
      page('deliveries', t.deliveriesTab, 'deliveries'),
      page('wiki', t.wikiTab, 'wiki'),
      page('xray', t.tabXray, 'xray'),
      page('usage', t.usageTab, 'usage'),
      page('efficiency', t.tabEfficiency, 'efficiency'),
      page('settings', t.settingsTitle, 'sliders'),
      action('nova', t.newFreeChat, 'new-chat', () => onChat('nova')),
      action('fluxo', t.flowNew, 'flows', () => onGo('flows')),
      action('projeto', t.addProject, 'plus', () => onChat('bots')),
      action('canal', t.channelNewNav, 'hash', () => onChat('canal:novo')),
      action('grupo', t.askMany, 'ask-many', () => onChat('grupo')),
      ...projects.map<Item>((bot) => ({
        key: `proj-${bot.name}`,
        group: t.projects,
        label: bot.name,
        hint: realBots.filter((b) => b.name === bot.name).map((b) => AGENT_LABEL[b.agent]).join(', '),
        icon: <Avatar agent={faceForBot(bot)} size={18} state="still" />,
        run: () => onChat(`bot:${bot.id}`),
      })),
      ...bots.flatMap((bot) =>
        bot.threads.map<Item>((thread) => ({
          key: `conv-${bot.id}-${thread.id}`,
          group: t.paletteConversations,
          label: thread.title,
          hint: bot.free ? AGENT_LABEL[bot.agent] : `${bot.name} · ${AGENT_LABEL[bot.agent]}`,
          icon: <Avatar agent={faceForBot(bot)} size={18} state="still" />,
          run: () => (bot.free ? onChat(`conversa:${bot.id}/${thread.id}`) : onThread(bot.id, thread.id)),
        })),
      ),
      ...channels.map<Item>((channel) => ({ key: `canal-${channel.id}`, group: t.channels, label: `#${channel.name}`, icon: <Icon name="hash" />, run: () => onChat(`canal:${channel.id}`) })),
      ...flows.map<Item>((flow) => ({ key: `fluxo-${flow.id}`, group: t.flowsTab, label: flow.name, icon: <Icon name="flows" />, run: () => onFlow(flow.id) })),
    ]
  }, [bots, channels, flows, onGo, onChat, onThread, onFlow])
  const words = plain(query).split(/\s+/).filter(Boolean)
  // Without a query: the pages, the actions and the projects; with one, everything that matches.
  const shown = (words.length ? items.filter((item) => matches(item, words)) : items.filter((item) => !item.key.startsWith('conv-') && !item.key.startsWith('canal-') && !item.key.startsWith('fluxo-'))).slice(0, MAX_RESULTS)
  useEffect(() => setActive(0), [query])
  useEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' })
  }, [active])
  const pick = (item: Item | undefined) => {
    if (!item) return
    onClose()
    item.run()
  }
  const onKey = (event: React.KeyboardEvent) => {
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      setActive((i) => Math.min(i + 1, shown.length - 1))
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      setActive((i) => Math.max(i - 1, 0))
    } else if (event.key === 'Enter') {
      event.preventDefault()
      pick(shown[active])
    } else if (event.key === 'Escape') {
      event.preventDefault()
      onClose()
    }
  }
  return (
    <div className="paleta-fundo" onPointerDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className="paleta" role="dialog" aria-modal="true" aria-label={t.paletteLabel}>
        <div className="paleta-campo">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="11" cy="11" r="6.5" />
            <path d="M16 16l4 4" />
          </svg>
          <input
            autoFocus
            value={query}
            placeholder={t.palettePlaceholder}
            aria-label={t.palettePlaceholder}
            role="combobox"
            aria-expanded="true"
            aria-controls="paleta-lista"
            aria-activedescendant={shown[active] ? `paleta-${shown[active].key}` : undefined}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={onKey}
          />
          <kbd>esc</kbd>
        </div>
        <ul id="paleta-lista" ref={list} role="listbox" aria-label={t.paletteLabel}>
          {shown.map((item, i) => (
            <li key={item.key}>
              {(i === 0 || shown[i - 1].group !== item.group) && <span className="paleta-grupo">{item.group}</span>}
              <button
                id={`paleta-${item.key}`}
                type="button"
                role="option"
                aria-selected={i === active}
                tabIndex={-1}
                onPointerMove={() => setActive(i)}
                onClick={() => pick(item)}
              >
                <span className="paleta-icone">{item.icon}</span>
                <span className="paleta-rotulo">{item.label}</span>
                {item.hint && <small>{item.hint}</small>}
              </button>
            </li>
          ))}
          {shown.length === 0 && <li className="paleta-vazio">{t.paletteNothing.replace('{q}', query.trim())}</li>}
        </ul>
      </div>
    </div>
  )
}
