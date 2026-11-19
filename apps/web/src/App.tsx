import { useEffect, useState } from 'react'
import { fetchReport, sessionToken, type Report } from './api'
import { useBots } from './bots'
import { exchanges } from './exchanges'
import { useUnread } from './unread'
import { ExchangeView } from './components/chat/Exchange'
import { BotChat } from './components/BotChat'
import { Desk } from './components/Desk'
import { BotPicker } from './components/BotPicker'
import { Clock } from './components/Clock'
import { ContextRuler } from './components/ContextRuler'
import { EfficiencyView } from './components/Efficiency'
import { SettingsPage, type Tab as SettingsTab } from './components/SettingsPage'
import { AskMany } from './components/chat/AskMany'
import { BatchView } from './components/chat/BatchView'
import type { ChatNav } from './components/chat/nav'
import { TipToast } from './components/TipToast'
import { Hero } from './components/Hero'
import { RowsCard, SecretsCard } from './components/Lists'
import { type ChatView, Sidebar, type Tab } from './components/Sidebar'
import { period, t } from './i18n'
import { Onboarding } from './onboarding/Onboarding'
import { carriedRows, hours, longestStreak, plateDays, projectRows, reportedCost, secretTotals, totalTokens } from './model'

const REFRESH_MS = 30_000
const LIST_ROWS = 12
const SOURCE_LABELS: Record<string, string> = { claude: 'CLAUDE', codex: 'CODEX' }
const PATHS: Record<Tab, string[]> = {
  xray: ['/'],
  efficiency: ['/eficiencia', '/efficiency'],
  agents: ['/agentes', '/agents'],
  desk: ['/mesa', '/desk'],
  settings: ['/configuracoes', '/settings'],
}
const HEADINGS: Record<Tab, { title: string; subtitle: string }> = {
  xray: { title: t.title, subtitle: t.subtitle },
  efficiency: { title: t.dietTitleMain, subtitle: t.dietSubtitle },
  agents: { title: t.botsTitle, subtitle: t.botsHint },
  desk: { title: t.deskTitle, subtitle: t.deskSubtitle },
  settings: { title: t.settingsTitle, subtitle: t.settingsSubtitle },
}

const BOTS_PATH = '/bots'
const EXCHANGE_PATH = '/trocas'
const GROUP_PATH = '/grupos'
const SETUP_PATHS = ['/boas-vindas', '/welcome']

function chatFromPath(): ChatView {
  const path = window.location.pathname
  if (path === BOTS_PATH) return 'bots'
  if (path === GROUP_PATH) return 'grupo'
  if (path.startsWith(`${GROUP_PATH}/`)) return `grupo:${path.slice(GROUP_PATH.length + 1)}`
  if (path.startsWith(`${EXCHANGE_PATH}/`)) return `troca:${decodeURIComponent(path.slice(EXCHANGE_PATH.length + 1))}`
  if (path.startsWith(`${BOTS_PATH}/`)) return `bot:${path.slice(BOTS_PATH.length + 1)}`
  return 'bots'
}

function chatPath(view: ChatView): string {
  if (view === 'bots') return BOTS_PATH
  if (view === 'grupo') return GROUP_PATH
  if (view.startsWith('grupo:')) return `${GROUP_PATH}/${view.slice(6)}`
  if (view.startsWith('bot:')) return `${BOTS_PATH}/${view.slice(4)}`
  return `${EXCHANGE_PATH}/${encodeURIComponent(view.slice(6))}`
}

function tabFromPath(): Tab {
  const path = window.location.pathname
  if (path.startsWith(BOTS_PATH) || path.startsWith(EXCHANGE_PATH) || path.startsWith(GROUP_PATH)) return 'agents'
  return (Object.keys(PATHS) as Tab[]).find((tab) => tab !== 'xray' && PATHS[tab].includes(path)) ?? 'xray'
}

type State = { kind: 'loading' } | { kind: 'failed'; message: string } | { kind: 'ready'; report: Report; at: Date }

export function App() {
  const [state, setState] = useState<State>({ kind: 'loading' })
  const [version, setVersion] = useState(0)
  const [setupOpen, setSetupOpen] = useState(() => SETUP_PATHS.includes(window.location.pathname))

  useEffect(() => {
    const token = sessionToken()
    if (!token) {
      setState({ kind: 'failed', message: t.noToken })
      return
    }
    let alive = true
    const load = async () => {
      try {
        const report = await fetchReport(token)
        if (alive) setState({ kind: 'ready', report, at: new Date() })
      } catch (error: unknown) {
        const message = error instanceof Error ? error.message : String(error)
        // Keep showing the last good report if a refresh fails.
        if (alive) setState((previous) => (previous.kind === 'ready' ? previous : { kind: 'failed', message }))
      }
    }
    void load()
    const timer = window.setInterval(() => void load(), REFRESH_MS)
    return () => {
      alive = false
      window.clearInterval(timer)
    }
  }, [version])

  if (state.kind === 'loading') return <div className="tela-estado">{t.loading}</div>
  if (state.kind === 'failed') return <div className="tela-estado">{`${t.failed}: ${state.message}`}</div>
  if (!state.report.setup.done || setupOpen) {
    const finish = (firstBot: string | null) => {
      history.pushState(null, '', firstBot ? `${BOTS_PATH}/${firstBot}` : '/')
      // Done now, before the next report arrives with it.
      setState({ ...state, report: { ...state.report, setup: { done: true } } })
      setSetupOpen(false)
      setVersion((v) => v + 1)
    }
    return <Onboarding onFinish={finish} />
  }
  const openSetup = () => {
    history.pushState(null, '', SETUP_PATHS[0])
    setSetupOpen(true)
  }
  return <Dashboard report={state.report} at={state.at} onChanged={() => setVersion((v) => v + 1)} onSetup={openSetup} />
}

interface DashboardProps {
  report: Report
  at: Date
  onChanged: () => void
  onSetup: () => void
}

function Dashboard({ report, at, onChanged, onSetup }: DashboardProps) {
  const [tab, setTab] = useState<Tab>(tabFromPath)
  const [chat, setChat] = useState<ChatView>(chatFromPath)
  const [focus, setFocus] = useState<{ bot: string; thread: string; ms: number } | null>(null)
  // Which Settings tab opens: Connections from the menu, another when a chat card sends the person there.
  const [settingsTab, setSettingsTab] = useState<SettingsTab>('conexoes')
  const bots = useBots(report.bots)
  useEffect(() => {
    const onBack = () => {
      setTab(tabFromPath())
      setChat(chatFromPath())
    }
    window.addEventListener('popstate', onBack)
    return () => window.removeEventListener('popstate', onBack)
  }, [])
  const go = (next: Tab) => {
    if (next !== tab) history.pushState(null, '', PATHS[next][0])
    setTab(next)
  }
  const openChat = (view: ChatView) => {
    const path = chatPath(view)
    if (path !== window.location.pathname) history.pushState(null, '', path)
    setChat(view)
    setTab('agents')
  }
  const sources = [...report.sources].sort((a, b) => totalTokens(b) - totalTokens(a))
  const main = sources[0]
  if (!main) return <div className="tela-estado">{t.noLogs}</div>
  const total = sources.reduce((sum, s) => sum + totalTokens(s), 0)
  const days = plateDays(report)
  const activeDays = days.filter((d) => d.tokens > 0).length
  const projects = projectRows(report, LIST_ROWS).map((row) => ({ ...row, share: row.value / total }))
  const openBot = chat.startsWith('bot:') ? bots.state.bots.find((b) => `bot:${b.id}` === chat) : undefined
  const openExchange = chat.startsWith('troca:')
    ? exchanges(bots.state.bots).find((x) => [x.a, x.b].sort().join('|') === chat.slice(6).split('|').sort().join('|'))
    : undefined
  const showExchange = (a: string, b: string) => openChat(`troca:${a}|${b}`)
  const openBatch = chat.startsWith('grupo:') ? bots.state.batches.find((b) => `grupo:${b.id}` === chat) : undefined
  const nav: ChatNav = {
    openBot: (id) => {
      setFocus(null)
      openChat(`bot:${id}`)
    },
    openExchange: showExchange,
    openBatch: (id) => openChat(`grupo:${id}`),
    askMany: () => openChat('grupo'),
    openMessage: (bot, thread, ms) => {
      setFocus({ bot, thread, ms })
      openChat(`bot:${bot}`)
    },
    add: () => openChat('bots'),
  }
  const unread = useUnread(bots.state.bots, tab === 'agents' && openBot ? openBot.id : null)
  const heading = HEADINGS[tab]
  return (
    <div className={`app ${tab === 'agents' && (openBot || openExchange || openBatch || chat === 'grupo') ? 'conversa-aberta' : ''}`}>
      <Sidebar
        tab={tab}
        chat={chat}
        bots={bots.state.bots}
        unread={unread}
        updatedAt={at}
        onTab={(next) => {
          setSettingsTab('conexoes')
          go(next)
        }}
        onChat={openChat}
      />
      <div className="principal">
        <div className="titulo">
          <h1>{heading.title}</h1>
          <p>{heading.subtitle}</p>
          {days.length > 0 && <span className="periodo">{period(days[0].date, days[days.length - 1].date, activeDays)}</span>}
        </div>
        {tab === 'agents' && openBot && <BotChat key={openBot.id} bot={openBot} bots={bots} unread={unread} focus={focus?.bot === openBot.id ? focus : null} onSettings={(tab) => {
          setSettingsTab(tab)
          go('settings')
        }} nav={nav} report={report} onChanged={onChanged} />}
        {tab === 'agents' && openExchange && (
          <ExchangeView exchange={openExchange} bots={bots} unread={unread} nav={nav} />
        )}
        {tab === 'agents' && chat === 'grupo' && <AskMany bots={bots} nav={nav} />}
        {tab === 'agents' && openBatch && <BatchView batch={openBatch} bots={bots} nav={nav} />}
        {tab === 'agents' && !openBot && !openExchange && !chat.startsWith('grupo') && <BotPicker bots={bots} onOpen={(id) => openChat(`bot:${id}`)} onAskMany={() => openChat('grupo')} />}
        {tab === 'desk' && <Desk bots={bots} nav={nav} />}
        {tab === 'efficiency' && <EfficiencyView report={report} onChanged={onChanged} />}
        {tab === 'settings' && (
          <SettingsPage bots={bots} onOpenBot={(id) => openChat(`bot:${id}`)} onAddBot={() => openChat('bots')} onRedoSetup={onSetup} initialTab={settingsTab} />
        )}
        {tab === 'xray' && (
          <main className="grade">
            <Hero
              total={total}
              cost={reportedCost(report)}
              days={days}
              streak={longestStreak(days)}
              bySource={sources.map((s) => ({ label: SOURCE_LABELS[s.overview.source] ?? s.overview.source, tokens: totalTokens(s) }))}
            />
            <Clock hours={hours(report)} />
            <ContextRuler sources={sources} main={main} onCut={() => go('efficiency')} />
            <RowsCard className="carregado" title={t.carried} hint={t.carriedHint} rows={carriedRows(report, LIST_ROWS)} />
            <RowsCard className="projetos" title={t.projects} hint={t.projectsHint} rows={projects} />
            <SecretsCard totals={secretTotals(report)} />
          </main>
        )}
      </div>
      <TipToast bots={bots} report={report} nav={nav} raised={tab === 'agents'} />
    </div>
  )
}
