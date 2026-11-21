import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { IS_REMOTE, type Bot, type BotAgent, type BotsState, type CliInstall, type PickedProject, fetchInstalls, installCli, pickProjectFolder } from '../api'
import { AGENT_LABEL, AGENT_SHORT } from '../avatars'
import type { Bots } from '../bots'
import { locale, t } from '../i18n'
import { AgentLogo } from './AgentLogo'

/** Top candidates shown before typing a query; searching reaches every folder. */
const SUGGESTED_LIMIT = 5
/** How often the modal checks a running Python install. */
const INSTALL_POLL_MS = 2500
const PYTHON_ID = 'python'
const PYTHON_URL = 'https://www.python.org/downloads/'
const PROJECT_NAME_RE = /^[A-Za-z0-9_][A-Za-z0-9_-]{0,59}$/
const FOCUSABLE = 'button:not(:disabled), input:not(:disabled), a[href], [tabindex]:not([tabindex="-1"])'

interface CandidateRow {
  project: string
  name: string
  usage: Partial<Record<BotAgent, number>>
  lastMs: number | null
}

const normalizePath = (p: string) => p.replace(/\\/g, '/')
const inProject = (bot: Bot, project: string) => normalizePath(bot.dir).endsWith(`/${normalizePath(project)}`)
const lastLine = (log: string) => log.trim().split('\n').pop() ?? ''

/** Deduplicates candidate entries per project folder and drops folders that already have a bot. */
function candidateRows(state: BotsState): CandidateRow[] {
  const byProject = new Map<string, CandidateRow>()
  for (const c of state.candidates) {
    const row = byProject.get(c.project) ?? { project: c.project, name: c.name, usage: {}, lastMs: c.last_ms > 0 ? c.last_ms : null }
    const usage = c.agent && c.requests > 0 ? { ...row.usage, [c.agent]: c.requests } : row.usage
    const lastMs = c.last_ms > 0 ? Math.max(row.lastMs ?? 0, c.last_ms) : row.lastMs
    byProject.set(c.project, { ...row, usage, lastMs })
  }
  return [...byProject.values()].filter((row) => !state.bots.some((b) => inProject(b, row.project)))
}

function totalUsage(row: CandidateRow): number {
  return Object.values(row.usage).reduce((sum, n) => sum + (n ?? 0), 0)
}

function topAgentOf(row: CandidateRow): BotAgent | null {
  let best: BotAgent | null = null
  let bestCount = 0
  for (const [agent, count] of Object.entries(row.usage) as [BotAgent, number][]) {
    if (count > bestCount) {
      best = agent
      bestCount = count
    }
  }
  return best
}

interface AddProjectModalProps {
  bots: Bots
  onOpen: (id: string) => void
  onClose: () => void
}

/** Overlay dialog to add an existing folder, create a new project, or start a template agent. */
export function AddProjectModal({ bots, onOpen, onClose }: AddProjectModalProps) {
  const dialogRef = useRef<HTMLDivElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const [query, setQuery] = useState('')
  const [choosing, setChoosing] = useState<string | null>(null)
  const [localFolder, setLocalFolder] = useState<PickedProject | null>(null)
  const [folderAgent, setFolderAgent] = useState<BotAgent | null>(null)
  const [pickingFolder, setPickingFolder] = useState(false)
  const [openingFolder, setOpeningFolder] = useState(false)
  const [folderError, setFolderError] = useState<string | null>(null)
  const [newName, setNewName] = useState('')
  const [newAgent, setNewAgent] = useState<BotAgent | null>(null)
  const [newCreating, setNewCreating] = useState(false)
  const [rsiOpen, setRsiOpen] = useState(false)
  const [rsiName, setRsiName] = useState(t.rsiDefaultName)
  const [rsiAgent, setRsiAgent] = useState<BotAgent | null>(null)
  const [rsiCreating, setRsiCreating] = useState(false)
  const [pythonJob, setPythonJob] = useState<CliInstall | null>(null)
  const [pythonError, setPythonError] = useState<string | null>(null)

  const agents = bots.state.agents.map((a) => a.id)
  const chosenFolderAgent = folderAgent && agents.includes(folderAgent) ? folderAgent : agents[0] ?? null
  const chosenNewAgent = newAgent && agents.includes(newAgent) ? newAgent : agents[0] ?? null
  const chosenRsiAgent = rsiAgent && agents.includes(rsiAgent) ? rsiAgent : agents[0] ?? null
  const hasPython = Boolean(bots.state.rsi_python)
  const canInstallPython = Boolean(bots.state.rsi_python_can_install)
  const pythonInstalling = pythonJob?.state === 'running'
  const validNewName = PROJECT_NAME_RE.test(newName.trim())

  const allCandidates = candidateRows(bots.state)
  const needle = query.trim().toLowerCase()
  const ranked = [...allCandidates].sort((a, b) => totalUsage(b) - totalUsage(a) || (b.lastMs ?? 0) - (a.lastMs ?? 0) || a.name.localeCompare(b.name))
  const filtered = needle ? ranked.filter((row) => `${row.name} ${row.project}`.toLowerCase().includes(needle)) : ranked
  const shown = needle ? filtered : filtered.slice(0, SUGGESTED_LIMIT)

  // Focus the search field on open, trap Tab inside the dialog, and close on Esc.
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    searchRef.current?.focus()
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        onClose()
        return
      }
      if (event.key !== 'Tab' || !dialogRef.current) return
      const nodes = [...dialogRef.current.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null)
      if (nodes.length === 0) return
      const first = nodes[0]
      const last = nodes[nodes.length - 1]
      const active = document.activeElement
      if (event.shiftKey) {
        if (active === first || !dialogRef.current.contains(active)) {
          event.preventDefault()
          last.focus()
        }
      } else if (active === last || !dialogRef.current.contains(active)) {
        event.preventDefault()
        first.focus()
      }
    }
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('keydown', onKeyDown)
      previous?.focus()
    }
  }, [onClose])

  // Pick up an in-progress Python install if the person opens the modal while it runs.
  useEffect(() => {
    if (IS_REMOTE || hasPython) return undefined
    let active = true
    fetchInstalls()
      .then((jobs) => {
        if (active && jobs[PYTHON_ID]) setPythonJob(jobs[PYTHON_ID])
      })
      .catch(() => undefined)
    return () => {
      active = false
    }
  }, [hasPython])

  // Follow the Python installer until it finishes, then refresh bots so the RSI template unlocks.
  useEffect(() => {
    if (!pythonInstalling) return undefined
    const timer = window.setInterval(() => {
      fetchInstalls()
        .then((jobs) => {
          const next = jobs[PYTHON_ID] ?? null
          setPythonJob(next)
          if (next && next.state === 'done') bots.refresh()
        })
        .catch(() => undefined)
    }, INSTALL_POLL_MS)
    return () => window.clearInterval(timer)
  }, [pythonInstalling, bots])

  const activate = async (row: CandidateRow, agent: BotAgent) => {
    setChoosing(null)
    const before = new Set(bots.state.bots.map((b) => b.id))
    const next = await bots.act('activate', { project: row.project, agent })
    const created = next?.bots.find((b) => !before.has(b.id))
    if (created) {
      onClose()
      onOpen(created.id)
    }
  }

  const browseFolders = async () => {
    setFolderError(null)
    setPickingFolder(true)
    try {
      const folder = await pickProjectFolder()
      if (folder) setLocalFolder(folder)
    } catch (failure: unknown) {
      setFolderError(failure instanceof Error ? failure.message : String(failure))
    } finally {
      setPickingFolder(false)
    }
  }

  const openLocalFolder = async () => {
    if (!localFolder || !chosenFolderAgent || openingFolder) return
    setOpeningFolder(true)
    try {
      const next = await bots.act('activate_folder', { project: localFolder.dir, agent: chosenFolderAgent })
      const bot = next?.bots.find((b) => b.agent === chosenFolderAgent && b.dir === localFolder.dir)
      if (bot) {
        onClose()
        onOpen(bot.id)
      }
    } finally {
      setOpeningFolder(false)
    }
  }

  const createNewProject = async () => {
    const clean = newName.trim()
    if (!chosenNewAgent || !PROJECT_NAME_RE.test(clean) || newCreating) return
    setNewCreating(true)
    const before = new Set(bots.state.bots.map((b) => b.id))
    try {
      const next = await bots.act('create_project', { name: clean, agent: chosenNewAgent })
      const created = next?.bots.find((b) => !before.has(b.id))
      if (created) {
        setNewName('')
        onClose()
        onOpen(created.id)
      }
    } finally {
      setNewCreating(false)
    }
  }

  const createRsi = async () => {
    if (!chosenRsiAgent || !hasPython || !rsiName.trim() || rsiCreating) return
    setRsiCreating(true)
    const before = new Set(bots.state.bots.map((b) => b.id))
    try {
      const next = await bots.act('rsi_create', { name: rsiName.trim(), agent: chosenRsiAgent, lang: locale })
      const created = next?.bots.find((b) => !before.has(b.id))
      if (created) {
        onClose()
        onOpen(created.id)
      }
    } finally {
      setRsiCreating(false)
    }
  }

  const startPythonInstall = async () => {
    if (pythonInstalling) return
    setPythonError(null)
    try {
      const jobs = await installCli(PYTHON_ID)
      setPythonJob(jobs[PYTHON_ID] ?? null)
    } catch (failure: unknown) {
      setPythonError(failure instanceof Error ? failure.message : String(failure))
    }
  }

  const orderedAgents = (top: BotAgent | null) => (top && agents.includes(top) ? [top, ...agents.filter((a) => a !== top)] : agents)
  const pickRow = (row: CandidateRow) => (agents.length === 1 ? void activate(row, agents[0]) : setChoosing(choosing === row.project ? null : row.project))

  return createPortal(
    <div className="add-proj-fundo" onPointerDown={(event) => event.target === event.currentTarget && onClose()}>
      <div ref={dialogRef} className="add-proj-painel" role="dialog" aria-modal="true" aria-label={t.addProject}>
        <div className="add-proj-busca">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="11" cy="11" r="6.5" />
            <path d="M16 16l4 4" />
          </svg>
          <input
            ref={searchRef}
            value={query}
            placeholder={t.searchProjects}
            aria-label={t.searchProjects}
            onChange={(event) => setQuery(event.target.value)}
          />
          {query && (
            <button type="button" className="icone-btn" aria-label={t.clearSearch} title={t.clearSearch} onClick={() => setQuery('')}>
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          )}
          <button type="button" className="icone-btn" aria-label={t.close} title={t.close} onClick={onClose}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M6 6l12 12M18 6L6 18" />
            </svg>
          </button>
        </div>

        <div className="add-proj-corpo">
          {agents.length === 0 && <p className="nota">{t.noAgentsOn}</p>}
          {bots.error && <p className="nota failed">{bots.error}</p>}

          {!IS_REMOTE && (
            <section className="add-proj-secao">
              <button type="button" className="projeto-acao" disabled={pickingFolder || openingFolder} onClick={() => void browseFolders()}>
                {pickingFolder ? t.choosingProjectFolder : t.chooseProjectFolder}
              </button>
              {folderError && <p className="nota failed" role="alert">{folderError}</p>}
              {localFolder && (
                <div className="add-proj-novo">
                  <div className="add-proj-modelo-topo">
                    <span className="add-proj-nome" title={localFolder.dir}><b>{localFolder.name}</b></span>
                    <button type="button" className="projeto-acao cheio" disabled={!chosenFolderAgent || pickingFolder || openingFolder} onClick={() => void openLocalFolder()}>
                      {t.addAction}
                    </button>
                  </div>
                  {agents.length > 1 && (
                    <span className="projeto-ias" role="radiogroup" aria-label={t.pickAi}>
                      {agents.map((agent) => (
                        <button
                          key={agent}
                          type="button"
                          role="radio"
                          aria-checked={chosenFolderAgent === agent}
                          className={`add-proj-pill ${chosenFolderAgent === agent ? 'ativo' : ''}`}
                          disabled={openingFolder}
                          onClick={() => setFolderAgent(agent)}
                        >
                          <AgentLogo agent={agent} size={13} />
                          <span>{AGENT_SHORT[agent]}</span>
                        </button>
                      ))}
                    </span>
                  )}
                </div>
              )}
            </section>
          )}

          <section className="add-proj-secao">
            <h3 className="add-proj-titulo">{t.suggestedProjects}</h3>
            {shown.length === 0 ? (
              <p className="nota">{t.noCandidates}</p>
            ) : (
              <ul className="add-proj-lista">
                {shown.map((row) => {
                  const top = topAgentOf(row)
                  return (
                    <li key={row.project} className="add-proj-linha">
                      <span className="add-proj-nome" title={row.project}>
                        <b>{row.name}</b>
                        {needle && <small>{row.project}</small>}
                      </span>
                      {choosing === row.project ? (
                        <span className="add-proj-ias" role="group" aria-label={t.pickAi}>
                          {orderedAgents(top).map((agent) => (
                            <button key={agent} type="button" className="acao-texto" onClick={() => void activate(row, agent)}>
                              {AGENT_SHORT[agent]}
                            </button>
                          ))}
                          <button type="button" className="lado-link" onClick={() => setChoosing(null)}>
                            {t.cancel}
                          </button>
                        </span>
                      ) : (
                        <>
                          {top && (
                            <span className="add-proj-ia">
                              <AgentLogo agent={top} size={14} />
                              <span>{AGENT_SHORT[top]}</span>
                            </span>
                          )}
                          <button type="button" className="acao-texto" disabled={agents.length === 0} onClick={() => pickRow(row)}>
                            {t.addAction}
                          </button>
                        </>
                      )}
                    </li>
                  )
                })}
              </ul>
            )}
            {!needle && filtered.length > shown.length && <p className="nota">{t.moreProjects.replace('{n}', String(filtered.length - shown.length))}</p>}
          </section>

          {!IS_REMOTE && (
            <section className="add-proj-secao">
              <h3 className="add-proj-titulo">{t.newFolderSection}</h3>
              <form
                className="add-proj-novo"
                onSubmit={(event) => {
                  event.preventDefault()
                  void createNewProject()
                }}
              >
                <div className="add-proj-novo-linha">
                  <input
                    value={newName}
                    maxLength={60}
                    placeholder={t.newProjectPlaceholder}
                    aria-label={t.newFolderSection}
                    onChange={(event) => setNewName(event.target.value.replace(/[^A-Za-z0-9_-]/g, ''))}
                  />
                  <button type="submit" className="projeto-acao cheio" disabled={!chosenNewAgent || !validNewName || newCreating}>
                    {newCreating ? t.newProjectCreating : t.createShort}
                  </button>
                </div>
                <small className="rsi-caminho">{newName.trim() ? t.rsiPathPreview.replace('{name}', newName.trim()) : t.newProjectNameHint}</small>
                {agents.length > 1 && (
                  <span className="projeto-ias" role="radiogroup" aria-label={t.pickAi}>
                    {agents.map((agent) => (
                      <button
                        key={agent}
                        type="button"
                        role="radio"
                        aria-checked={chosenNewAgent === agent}
                        className={`add-proj-pill ${chosenNewAgent === agent ? 'ativo' : ''}`}
                        onClick={() => setNewAgent(agent)}
                      >
                        <AgentLogo agent={agent} size={13} />
                        <span>{AGENT_SHORT[agent]}</span>
                      </button>
                    ))}
                  </span>
                )}
              </form>
            </section>
          )}

          {!IS_REMOTE && (
            <section className="add-proj-secao add-proj-modelos">
              <h3 className="add-proj-titulo">{t.templatesSection}</h3>
              <div className="add-proj-modelo-topo">
                <span className="add-proj-modelo-texto">
                  <b>{t.rsiTitle}</b>
                  <small>{t.rsiHint}</small>
                </span>
                <button
                  type="button"
                  className="acao-texto"
                  disabled={agents.length === 0 || !hasPython}
                  aria-expanded={rsiOpen}
                  onClick={() => setRsiOpen((open) => !open)}
                >
                  {rsiOpen ? t.close : t.rsiCreate}
                </button>
              </div>
              {!hasPython && (
                <div className="rsi-python">
                  <span className="nota">{t.rsiNoPython}</span>
                  {canInstallPython ? (
                    <button type="button" className="acao-texto" disabled={pythonInstalling} onClick={() => void startPythonInstall()}>
                      {pythonInstalling ? t.rsiInstallingPython : t.rsiInstallPython}
                    </button>
                  ) : (
                    <a className="acao-texto" href={PYTHON_URL} target="_blank" rel="noreferrer">
                      {t.rsiDownloadPython}
                    </a>
                  )}
                </div>
              )}
              {!hasPython && pythonInstalling && pythonJob?.log && <small className="rsi-caminho">{lastLine(pythonJob.log)}</small>}
              {!hasPython && (pythonJob?.state === 'failed' || pythonError) && (
                <div className="rsi-python">
                  <span className="nota">{pythonError ?? t.rsiPythonFailed}</span>
                  <a className="acao-texto" href={PYTHON_URL} target="_blank" rel="noreferrer">
                    {t.rsiDownloadPython}
                  </a>
                </div>
              )}
              {rsiOpen && hasPython && (
                <form
                  className="rsi-modelo-form"
                  onSubmit={(event) => {
                    event.preventDefault()
                    void createRsi()
                  }}
                >
                  <label className="rsi-campo">
                    <span>{t.rsiFolder}</span>
                    <input
                      value={rsiName}
                      maxLength={48}
                      placeholder={t.rsiDefaultName}
                      onChange={(event) => setRsiName(event.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, ''))}
                    />
                    <small className="rsi-caminho">{t.rsiPathPreview.replace('{name}', rsiName.trim() || t.rsiDefaultName)}</small>
                  </label>
                  {agents.length > 1 && (
                    <div className="rsi-campo">
                      <span>{t.pickAi}</span>
                      <span className="projeto-ias" role="radiogroup" aria-label={t.pickAi}>
                        {agents.map((agent) => (
                          <button
                            key={agent}
                            type="button"
                            role="radio"
                            aria-checked={chosenRsiAgent === agent}
                            className={`projeto-acao ${chosenRsiAgent === agent ? 'cheio' : ''}`}
                            onClick={() => setRsiAgent(agent)}
                          >
                            {AGENT_LABEL[agent]}
                          </button>
                        ))}
                      </span>
                    </div>
                  )}
                  <div className="rsi-acoes">
                    <button type="submit" className="projeto-acao cheio" disabled={!hasPython || !chosenRsiAgent || !rsiName.trim() || rsiCreating}>
                      {rsiCreating ? t.rsiCreating : t.rsiCreate}
                    </button>
                  </div>
                </form>
              )}
            </section>
          )}
        </div>

        <footer className="add-proj-rodape">{t.addProjectFooter}</footer>
      </div>
    </div>,
    document.body,
  )
}
