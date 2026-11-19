import { type ReactNode, useCallback, useEffect, useState } from 'react'
import { type GitHubState, IS_REMOTE, type SettingsState, fetchGitHub, fetchSettings, saveSettings } from '../api'
import { AGENT_LABEL, faceForBot, faceOf } from '../avatars'
import type { Bots } from '../bots'
import { t } from '../i18n'
import { PRESETS } from '../onboarding/presets'
import { TourDialog } from '../tour/TourDialog'
import { BotsPanel } from './settings/BotsPanel'
import { ConnectionTile, GitHubTile } from './settings/ConnectionTile'
import { type SetupStep, SetupHero, type StepId } from './settings/SetupHero'
import { PhonePanel } from './settings/PhonePanel'
import { WiresPanel } from './settings/WiresPanel'
import './settings-page.css'

export type Tab = 'conexoes' | 'bots' | 'fios' | 'celular'

const message = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure))
const count = (n: number, [one, many]: [string, string]) => `${n} ${n === 1 ? one : many}`

const ICONS: Record<Tab, ReactNode> = {
  conexoes: <path d="M9 7V3M15 7V3M7 7h10v4a5 5 0 0 1-10 0zM12 16v5" />,
  bots: <path d="M6 8h12a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2zM10 12v1M14 12v1M12 8V5" />,
  fios: <path d="M6 6a2 2 0 1 0 0 .01M18 18a2 2 0 1 0 0 .01M18 6a2 2 0 1 0 0 .01M7.5 7.5l9 9M8 6h8" />,
  celular: <path d="M9 2.5h6a2.5 2.5 0 0 1 2.5 2.5v14a2.5 2.5 0 0 1-2.5 2.5H9A2.5 2.5 0 0 1 6.5 19V5A2.5 2.5 0 0 1 9 2.5zM10.5 18.5h3" />,
}

interface SettingsPageProps {
  bots: Bots
  onOpenBot: (id: string) => void
  onAddBot: () => void
  onRedoSetup: () => void
  /** The tab to open on, when another page sent the person to one. */
  initialTab: Tab
}

/** Settings as the onboarding left them: the setup at a glance on top, then one part at a time. */
export function SettingsPage({ bots, onOpenBot, onAddBot, onRedoSetup, initialTab }: SettingsPageProps) {
  const [settings, setSettings] = useState<SettingsState | null>(null)
  const [github, setGithub] = useState<GitHubState | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [touring, setTouring] = useState(false)
  const [tab, setTab] = useState<Tab>(initialTab)

  const loadGithub = useCallback(() => {
    setGithub(null)
    fetchGitHub().then(setGithub).catch((failure: unknown) => setError(message(failure)))
  }, [])
  useEffect(() => {
    fetchSettings().then(setSettings).catch((failure: unknown) => setError(message(failure)))
    loadGithub()
  }, [loadGithub])

  const save = async (change: Record<string, unknown>) => {
    setError(null)
    try {
      setSettings(await saveSettings(change))
      // The bots page and pickers read the turned-on CLIs from the bots state.
      bots.refresh()
    } catch (failure: unknown) {
      setError(message(failure))
    }
  }

  const connections = settings?.connections ?? []
  // Ready first, then the ones that only need a sign-in, then the ones not on this machine.
  const rank = (installed: boolean, signedOut: boolean) => (!installed ? 2 : signedOut ? 1 : 0)
  const ordered = [...connections].sort((a, b) => rank(a.installed, a.logged_in === false) - rank(b.installed, b.logged_in === false))
  const ready = ordered.filter((c) => c.installed && c.logged_in !== false)
  const aisOn = connections.filter((c) => c.enabled && c.installed && c.logged_in !== false).length
  const projects = new Set(bots.state.bots.map((b) => b.name)).size
  const links = bots.state.links.length
  const steps: SetupStep[] = [
    { id: 'ais', label: t.stepAis, done: settings ? connections.some((c) => c.bots && c.enabled && c.installed && c.logged_in !== false) : null },
    { id: 'github', label: t.githubTitle, done: github ? github.logged_in : null },
    { id: 'bots', label: t.stepBots, done: bots.state.bots.length > 0 },
    ...(projects >= 2 ? [{ id: 'wires' as const, label: t.stepWires, done: links > 0 }] : []),
  ]
  const next = (step: StepId) => {
    if (step === 'bots') onAddBot()
    else if (step === 'wires') setTab('fios')
    else {
      setTab('conexoes')
      if (step === 'github') window.setTimeout(() => document.getElementById('cfg-github')?.scrollIntoView({ behavior: 'smooth', block: 'center' }), 0)
    }
  }
  // A count shows once it is known: no '0 connections' while the machine is still being checked.
  const tabs: { id: Tab; label: string; count: number | null }[] = [
    { id: 'conexoes', label: t.tabConnections, count: settings ? aisOn : null },
    { id: 'bots', label: t.tabBots, count: projects },
    { id: 'fios', label: t.tabWires, count: links },
    // Pairing and the tunnel are managed on this machine only.
    ...(IS_REMOTE ? [] : [{ id: 'celular' as const, label: t.tabPhone, count: null }]),
  ]
  const first = bots.state.bots[0]
  const face = first ? faceForBot(first) : faceOf(PRESETS[0].shape, PRESETS[0].color, PRESETS[0].name)

  return (
    <main className="cfg">
      <SetupHero
        steps={steps}
        summary={[count(aisOn, t.aisOn), count(projects, t.botsCount), count(links, t.wiresCount)].join(' · ')}
        face={face}
        onNext={next}
        onTour={() => setTouring(true)}
        onRedo={onRedoSetup}
      />
      <div className="cfg-abas" role="tablist" aria-label={t.settingsTitle}>
        {tabs.map((item) => (
          <button key={item.id} type="button" role="tab" aria-selected={tab === item.id} className={tab === item.id ? 'ativa' : ''} onClick={() => setTab(item.id)}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              {ICONS[item.id]}
            </svg>
            {item.label}
            {item.count !== null && <small>{item.count}</small>}
          </button>
        ))}
      </div>
      <div className="cfg-painel" role="tabpanel">
        {tab === 'conexoes' && (
          <div className="cfg-grade">
            {!settings && <p className="cfg-vazio">{t.probing}</p>}
            {ready.map((c) => (
              <ConnectionTile
                key={c.id}
                connection={c}
                defaults={settings?.defaults[c.id] ?? { model: null, effort: null }}
                onToggle={(on) => void save({ enabled: { [c.id]: on } })}
                onDefaults={(chosen) => void save({ defaults: { [c.id]: { model: chosen.model ?? '', effort: chosen.effort ?? '' } } })}
              />
            ))}
            {settings && <GitHubTile github={github} onCheck={loadGithub} />}
            {ordered
              .filter((c) => !ready.includes(c))
              .map((c) => (
                <ConnectionTile key={c.id} connection={c} defaults={{ model: null, effort: null }} onToggle={() => undefined} onDefaults={() => undefined} />
              ))}
          </div>
        )}
        {tab === 'bots' && <BotsPanel bots={bots} onOpenBot={onOpenBot} onAddBot={onAddBot} />}
        {tab === 'fios' && <WiresPanel bots={bots} />}
        {tab === 'celular' && !IS_REMOTE && <PhonePanel />}
        {(error ?? bots.error) && <p className="nota failed quebra">{error ?? bots.error}</p>}
      </div>
      {touring && <TourDialog face={face} agent={AGENT_LABEL[first?.agent ?? 'claude']} onClose={() => setTouring(false)} />}
    </main>
  )
}
