import { useEffect, useState } from 'react'
import { fetchReport, sessionToken, type Report } from './api'
import { Clock } from './components/Clock'
import { EfficiencyView } from './components/Efficiency'
import { ContextRuler } from './components/ContextRuler'
import { Hero } from './components/Hero'
import { RowsCard, SecretsCard } from './components/Lists'
import { nowStamp, period, t } from './i18n'
import {
  carriedRows,
  hours,
  longestStreak,
  plateDays,
  projectRows,
  reportedCost,
  secretTotals,
  totalTokens,
} from './model'

const REFRESH_MS = 30_000
const LIST_ROWS = 12
const SOURCE_LABELS: Record<string, string> = { claude: 'CLAUDE', codex: 'CODEX' }

type Tab = 'xray' | 'efficiency'

const EFFICIENCY_PATHS = ['/eficiencia', '/efficiency']

function tabFromPath(): Tab {
  return EFFICIENCY_PATHS.includes(window.location.pathname) ? 'efficiency' : 'xray'
}

type State = { kind: 'loading' } | { kind: 'failed'; message: string } | { kind: 'ready'; report: Report; at: Date }

export function App() {
  const [state, setState] = useState<State>({ kind: 'loading' })
  const [version, setVersion] = useState(0)

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
  return <Dashboard report={state.report} at={state.at} onChanged={() => setVersion((v) => v + 1)} />
}

interface DashboardProps {
  report: Report
  at: Date
  onChanged: () => void
}

function Dashboard({ report, at, onChanged }: DashboardProps) {
  const [tab, setTab] = useState<Tab>(tabFromPath)
  useEffect(() => {
    const onBack = () => setTab(tabFromPath())
    window.addEventListener('popstate', onBack)
    return () => window.removeEventListener('popstate', onBack)
  }, [])
  const go = (next: Tab) => {
    history.pushState(null, '', next === 'efficiency' ? EFFICIENCY_PATHS[0] : '/')
    setTab(next)
  }
  const sources = [...report.sources].sort((a, b) => totalTokens(b) - totalTokens(a))
  const main = sources[0]
  if (!main) return <div className="tela-estado">{t.noLogs}</div>
  const total = sources.reduce((sum, s) => sum + totalTokens(s), 0)
  const days = plateDays(report)
  const activeDays = days.filter((d) => d.tokens > 0).length
  const projects = projectRows(report, LIST_ROWS).map((row) => ({ ...row, share: row.value / total }))
  return (
    <div className="app">
      <header className="topo">
        <span className="marca">
          <Mark />
          raiox
        </span>
        <nav className="abas" aria-label="raiox">
          {(['xray', 'efficiency'] as const).map((id) => (
            <button key={id} className={tab === id ? 'ativo' : ''} aria-current={tab === id ? 'page' : undefined} onClick={() => go(id)}>
              {id === 'xray' ? t.tabXray : t.tabEfficiency}
            </button>
          ))}
        </nav>
        <span className="meta">
          <span>{nowStamp(at)}</span>
          <span className="estado">
            <i className="ponto ok" />
            {t.updated}
          </span>
        </span>
      </header>
      <div className="titulo">
        <h1>{tab === 'xray' ? t.title : t.dietTitleMain}</h1>
        <p>{tab === 'xray' ? t.subtitle : t.dietSubtitle}</p>
        {days.length > 0 && <span className="periodo">{period(days[0].date, days[days.length - 1].date, activeDays)}</span>}
      </div>
      {tab === 'efficiency' ? (
        <EfficiencyView report={report} onChanged={onChanged} />
      ) : (
      <main className="grade">
        <Hero
          total={total}
          cost={reportedCost(report)}
          days={days}
          streak={longestStreak(days)}
          bySource={sources.map((s) => ({ label: SOURCE_LABELS[s.overview.source] ?? s.overview.source, tokens: totalTokens(s) }))}
        />
        <Clock hours={hours(report)} />
        <ContextRuler sources={sources} main={main} />
        <RowsCard className="carregado" title={t.carried} hint={t.carriedHint} rows={carriedRows(report, LIST_ROWS)} />
        <RowsCard className="projetos" title={t.projects} rows={projects} />
        <SecretsCard totals={secretTotals(report)} />
      </main>
      )}
    </div>
  )
}

function Mark() {
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
