import { useEffect, useRef, useState } from 'react'
import {
  IS_REMOTE,
  type Bot,
  type ContextStatus,
  type Report,
  type Source,
  type WikiEntry,
  fetchSession,
  fetchWiki,
  projectBots,
  taskAction,
} from '../api'
import type { Bots } from '../bots'
import { compact, locale, t } from '../i18n'
import type { ChatNav } from './chat/nav'
import { Segmented } from './Segmented'
import { SelectChip } from './SelectChip'
import type { Tab } from './Sidebar'
import { BloatCard } from './efficiency/BloatCard'
import { CutsCard } from './efficiency/CutsCard'
import { KpiRow } from './efficiency/KpiRow'
import { LedgerCard } from './efficiency/LedgerCard'
import { LeversCard } from './efficiency/LeversCard'
import { SessionBento } from './efficiency/SessionBento'
import './efficiency/efficiency.css'

const SOURCE_NAMES: Record<Source, string> = { claude: 'Claude Code', codex: 'Codex' }

interface EfficiencyViewProps {
  report: Report
  bots: Bots
  nav: ChatNav
  onGo: (tab: Tab) => void
  onChanged: () => void
}

interface Target {
  dir: string
  name: string
  bot?: string
}

/** Where cuts and quick levers act: the project raiox opened in plus every active project bot. */
function targetsOf(report: Report, liveBots: Bot[]): Target[] {
  const hereBot = liveBots.find((b) => b.dir === report.project_dir)
  const here: Target = {
    dir: report.project_dir,
    name: hereBot?.name ?? report.project_dir.split('/').pop() ?? report.project_dir,
    bot: hereBot?.id,
  }
  const others = [
    ...new Map(
      liveBots
        .filter((b) => b.dir !== report.project_dir)
        .map((b) => [b.dir, { dir: b.dir, name: b.name, bot: b.id }]),
    ).values(),
  ]
  return [here, ...others.sort((a, b) => a.name.localeCompare(b.name))]
}

export function EfficiencyView({ report, bots, nav, onGo, onChanged }: EfficiencyViewProps) {
  const [source, setSource] = useState<Source>(report.sources[0]?.overview.source ?? 'claude')
  const liveBots = projectBots(bots.state.bots)
  const targets = targetsOf(report, liveBots)
  const [targetDir, setTargetDir] = useState(targets[0].dir)
  const target = targets.find((x) => x.dir === targetDir) ?? targets[0]
  const targetBot = target.bot ? liveBots.find((b) => b.id === target.bot) : liveBots.find((b) => b.dir === target.dir)
  const botByDir = new Map(liveBots.map((b) => [b.dir, b.id]))

  const selected = report.sources.find((s) => s.overview.source === source) ?? report.sources[0]
  const inTarget = (file: string) => file.startsWith(`${target.dir}/`)
  const openChanges = report.changes.filter((c) => c.undone_ms === null && inTarget(c.file))
  const lessons = report.changes.filter((c) => c.lesson !== null).length
  const applicable = report.applicable[source] ?? []

  const status = useProjectSession(targetBot?.id)
  const wikiPages = useWikiList()

  const [highlight, setHighlight] = useState<'sim' | 'ledger' | null>(null)
  const [auditing, setAuditing] = useState(false)
  const [createdTasks, setCreatedTasks] = useState<Record<string, boolean>>({})
  const [actionError, setActionError] = useState<string | null>(null)
  const pulseTimer = useRef<number>(0)

  useEffect(() => () => window.clearTimeout(pulseTimer.current), [])

  if (!selected) return null

  const focusSection = (sectionId: string, flag: 'sim' | 'ledger') => {
    const el = document.getElementById(sectionId) ?? document.getElementById('efi-simulador')
    el?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    setHighlight(flag)
    window.clearTimeout(pulseTimer.current)
    pulseTimer.current = window.setTimeout(() => setHighlight(null), 1600)
  }

  const runAudit = async () => {
    if (!targetBot || auditing || targetBot.busy) return
    setActionError(null)
    setAuditing(true)
    try {
      if (!targetBot.auditor?.enabled) {
        if (IS_REMOTE) return
        const enabled = await bots.act('auditor', { id: targetBot.id, enabled: true })
        if (!enabled) return
      }
      const next = await bots.act('audit_run', { id: targetBot.id, lang: locale })
      const updated = next?.bots.find((b) => b.id === targetBot.id)
      const threadId = updated?.auditor?.thread
      if (threadId) nav.openMessage(targetBot.id, threadId, 0)
    } finally {
      setAuditing(false)
    }
  }

  const newCleanChat = async () => {
    if (!targetBot) {
      onGo('agents')
      return
    }
    setActionError(null)
    const next = await bots.act('thread_new', { id: targetBot.id })
    const created = next?.bots.find((b) => b.id === targetBot.id)?.threads.at(-1)
    if (created) nav.openMessage(targetBot.id, created.id, 0)
    else nav.openBot(targetBot.id)
  }

  const createTask = async (kind: 'shots' | 'startup') => {
    if (!targetBot) return
    const key = `${targetBot.id}:${kind}`
    if (createdTasks[key]) return
    setActionError(null)
    const title = kind === 'shots' ? t.efiTaskShotsTitle : t.efiTaskStartupTitle
    const detail = kind === 'shots' ? t.efiTaskShotsDetail : t.efiTaskStartupDetail
    try {
      await taskAction('add', { bot: targetBot.id, title, detail })
      setCreatedTasks((prev) => ({ ...prev, [key]: true }))
    } catch (error: unknown) {
      setActionError(error instanceof Error ? error.message : String(error))
    }
  }

  const taskFlags = {
    shots: Boolean(targetBot && createdTasks[`${targetBot.id}:shots`]),
    startup: Boolean(targetBot && createdTasks[`${targetBot.id}:startup`]),
  }

  const metaBar =
    openChanges.length > 1
      ? t.efiMetaBar.replace('{calls}', compact(selected.efficiency.context.calls)).replace('{active}', String(openChanges.length))
      : openChanges.length === 1
        ? t.efiMetaBarOne.replace('{calls}', compact(selected.efficiency.context.calls))
        : t.efiMetaBarNone.replace('{calls}', compact(selected.efficiency.context.calls))

  return (
    <main className="eficiencia-grid">
      <div className="efi-barra">
        <div className="efi-barra-esq">
          {report.sources.length > 1 && (
            <Segmented
              label={t.dietaTitle}
              value={source}
              onChange={setSource}
              options={report.sources.map((s) => ({ id: s.overview.source, label: SOURCE_NAMES[s.overview.source] }))}
            />
          )}
          <span className="efi-meta">{metaBar}</span>
        </div>
        <div className="efi-barra-dir">
          {targets.length > 1 && (
            <SelectChip
              className="alvo-projeto"
              label={t.applyIn}
              prefix={t.applyIn}
              value={target.dir}
              align="right"
              options={targets.map((x) => ({ value: x.dir, label: x.name }))}
              onChange={setTargetDir}
            />
          )}
        </div>
      </div>

      {(actionError || bots.error) && (
        <div className="efi-barra">
          <span className="nota failed">{actionError ?? bots.error}</span>
        </div>
      )}

      <KpiRow
        selected={selected}
        status={status}
        targetName={target.name}
        targetBot={targetBot}
        openChanges={openChanges}
        outcomes={report.outcomes}
        lessons={lessons}
        auditing={auditing}
        onJumpSimulator={() => focusSection('efi-simulador', 'sim')}
        onJumpCuts={() => focusSection('efi-simulador', 'sim')}
        onJumpLedger={() => focusSection('efi-historico', 'ledger')}
        onRunAudit={() => void runAudit()}
        onNewCleanChat={() => void newCleanChat()}
        onOpenChat={() => (targetBot ? nav.openBot(targetBot.id) : onGo('agents'))}
        onOpenWiki={() => onGo('wiki')}
      />

      <CutsCard
        selected={selected}
        source={source}
        targetDir={target.dir}
        targetName={target.name}
        targetBot={targetBot}
        applicable={applicable}
        openChanges={openChanges}
        outcomes={report.outcomes}
        highlighted={highlight === 'sim'}
        auditing={auditing}
        createdTasks={taskFlags}
        onChanged={onChanged}
        onRunAudit={() => void runAudit()}
        onNewCleanChat={() => void newCleanChat()}
        onOpenWiki={() => onGo('wiki')}
        onOpenBoard={() => onGo('desk')}
        onOpenChats={() => onGo('agents')}
        onCreateTask={(kind) => void createTask(kind)}
      />

      <SessionBento
        status={status}
        targetName={target.name}
        targetBot={targetBot}
        buckets={selected.efficiency.buckets}
        onNewCleanChat={() => void newCleanChat()}
        onOpenChat={() => (targetBot ? nav.openBot(targetBot.id) : onGo('agents'))}
      />

      <BloatCard
        carried={selected.efficiency.carried}
        costShares={selected.efficiency.cost_shares}
        applicable={applicable}
        openChanges={openChanges}
        targetBot={targetBot}
        auditing={auditing}
        shotsTaskCreated={taskFlags.shots}
        onFocusTrim={() => focusSection('efi-corte-trim-tool-output', 'sim')}
        onCreateShotsTask={() => void createTask('shots')}
        onRunAudit={() => void runAudit()}
        onOpenWiki={() => onGo('wiki')}
        onOpenBoard={() => onGo('desk')}
      />

      <LeversCard
        targetName={target.name}
        targetBot={targetBot}
        bots={bots}
        wikiPages={wikiPages}
        auditing={auditing}
        onRunAudit={() => void runAudit()}
        onOpenAuditThread={(threadId) => targetBot && nav.openMessage(targetBot.id, threadId, 0)}
        onOpenWiki={() => onGo('wiki')}
        onOpenBoard={() => onGo('desk')}
        onAddBot={() => nav.add()}
      />

      <LedgerCard
        changes={report.changes}
        outcomes={report.outcomes}
        targetDir={target.dir}
        botByDir={botByDir}
        highlighted={highlight === 'ledger'}
        onChanged={onChanged}
      />
    </main>
  )
}

/** Asks for a project's current session whenever the chosen project changes. */
function useProjectSession(bot: string | undefined): ContextStatus | null | undefined {
  const [status, setStatus] = useState<ContextStatus | null | undefined>(undefined)
  useEffect(() => {
    let alive = true
    setStatus(undefined)
    fetchSession(bot)
      .then((next) => alive && setStatus(next))
      .catch(() => alive && setStatus(null))
    return () => {
      alive = false
    }
  }, [bot])
  return status
}

/** Loads the team's shared wiki once so the fixed-memory lever shows real per-turn weight. */
function useWikiList(): WikiEntry[] | null {
  const [pages, setPages] = useState<WikiEntry[] | null>(null)
  useEffect(() => {
    let alive = true
    fetchWiki()
      .then((list) => alive && setPages(list.pages))
      .catch(() => alive && setPages([]))
    return () => {
      alive = false
    }
  }, [])
  return pages
}
