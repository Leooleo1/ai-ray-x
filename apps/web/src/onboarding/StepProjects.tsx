import { useState } from 'react'
import { type GitHubState, type LocalProject, type SetupState, cloneRepo, createSetupProject } from '../api'
import { compact } from '../i18n'
import { Frame } from './parts'
import { o } from './texts'

const VISIBLE = 8
/** Bots made in one go; more can be added later from the Bots page. */
export const MAX_PICK = 6
const PROJECT_NAME_RE = /^[A-Za-z0-9_][A-Za-z0-9_-]{0,59}$/

interface StepProjectsProps {
  setup: SetupState
  github: GitHubState | null
  selected: string[]
  onSelect: (selected: string[]) => void
  onSetup: (setup: SetupState) => void
  onError: (message: string) => void
  onBack: () => void
  onNext: () => void
  onSkip: () => void
}

function usage(p: LocalProject): string {
  const parts = [p.claude > 0 ? `Claude ${compact(p.claude)}` : '', p.codex > 0 ? `Codex ${compact(p.codex)}` : ''].filter(Boolean)
  return parts.length > 0 ? parts.join(' · ') : o.noUsage
}

export function StepProjects({ setup, github, selected, onSelect, onSetup, onError, onBack, onNext, onSkip }: StepProjectsProps) {
  const [query, setQuery] = useState('')
  const [newName, setNewName] = useState('')
  const [creating, setCreating] = useState(false)
  const needle = query.trim().toLowerCase()
  const matches = setup.projects.filter((p) => !needle || `${p.project} ${p.github ?? ''}`.toLowerCase().includes(needle))
  // Checked projects stay in view even when the search would hide them.
  const shown = needle ? matches : [...setup.projects.filter((p) => selected.includes(p.project)), ...matches.filter((p) => !selected.includes(p.project))].slice(0, Math.max(VISIBLE, selected.length))
  const hidden = needle ? 0 : setup.projects.length - shown.length
  const local = new Set(setup.projects.flatMap((p) => (p.github ? [p.github] : [])))
  const remote = (github?.repos ?? []).filter((r) => !local.has(r.repo) && (!needle || r.repo.toLowerCase().includes(needle)))
  const validNewName = PROJECT_NAME_RE.test(newName.trim())

  const toggle = (project: string) => {
    if (selected.includes(project)) onSelect(selected.filter((p) => p !== project))
    else if (selected.length < MAX_PICK) onSelect([...selected, project])
  }
  const clone = async (repo: string) => {
    try {
      onSetup(await cloneRepo(repo))
    } catch (failure: unknown) {
      onError(failure instanceof Error ? failure.message : String(failure))
    }
  }
  const create = async () => {
    const clean = newName.trim()
    if (!PROJECT_NAME_RE.test(clean) || creating) return
    setCreating(true)
    onError('')
    try {
      const res = await createSetupProject(clean)
      const exists = res.setup.projects.some((p) => p.project === res.project.project)
      onSetup(exists ? res.setup : { ...res.setup, projects: [res.project, ...res.setup.projects] })
      onSelect(selected.includes(res.project.project) ? selected : [...selected, res.project.project].slice(0, MAX_PICK))
      setNewName('')
    } catch (failure: unknown) {
      onError(failure instanceof Error ? failure.message : String(failure))
    } finally {
      setCreating(false)
    }
  }

  return (
    <Frame
      actions={
        <>
          <button className="bv-btn link" onClick={onBack}>
            {o.back}
          </button>
          <span className="bv-mono">{`${selected.length}/${MAX_PICK} ${o.checked}`}</span>
          <button className="bv-btn vidro bv-principal" onClick={onSkip}>
            {o.noBotsYet}
          </button>
          <button className="bv-btn cheio" disabled={selected.length === 0} onClick={onNext}>
            {o.continue}
          </button>
        </>
      }
    >
      {setup.projects.length === 0 && (
        <form
          className="bv-novo-projeto"
          onSubmit={(event) => {
            event.preventDefault()
            void create()
          }}
        >
          <div className="bv-novo-projeto-topo">
            <b>{o.newProjectTitle}</b>
            <small>{o.newProjectHint.replace('{name}', newName.trim() || o.newProjectPlaceholder)}</small>
          </div>
          <div className="bv-novo-projeto-linha">
            <input
              className="bv-busca"
              value={newName}
              maxLength={60}
              placeholder={o.newProjectPlaceholder}
              aria-label={o.newProjectTitle}
              onChange={(event) => setNewName(event.target.value.replace(/[^A-Za-z0-9_-]/g, ''))}
            />
            <button type="submit" className="bv-btn cheio" disabled={!validNewName || creating}>
              {creating ? o.newProjectCreating : o.newProjectCreate}
            </button>
          </div>
        </form>
      )}
      {(setup.projects.length > 0 || remote.length > 0) && (
        <input className="bv-busca" value={query} onChange={(event) => setQuery(event.target.value)} placeholder={o.search} aria-label={o.search} />
      )}
      {shown.length > 0 && (
        <ul className="bv-lista">
          {shown.map((p) => {
            const checked = selected.includes(p.project)
            return (
              <li key={p.project}>
                <label className={`bv-projeto ${checked ? 'marcado' : ''}`}>
                  <input type="checkbox" checked={checked} disabled={!checked && selected.length >= MAX_PICK} onChange={() => toggle(p.project)} />
                  <span className="bv-check" aria-hidden="true" />
                  <b>{p.name}</b>
                  <span className="bv-mono">{p.github ?? p.project}</span>
                  <span className="bv-mono bv-direita">{usage(p)}</span>
                </label>
              </li>
            )
          })}
        </ul>
      )}
      {hidden > 0 && <p className="bv-mono bv-mais">{`+${hidden} ${o.moreToSearch}`}</p>}
      {remote.length > 0 && (
        <>
          <span className="bv-grupo">{o.onlyGitHub}</span>
          <ul className="bv-lista">
            {remote.slice(0, VISIBLE).map((repo) => {
              const running = setup.clones.running.includes(repo.repo)
              const failed = setup.clones.failed[repo.repo]
              return (
                <li key={repo.repo} className="bv-repo">
                  <i className="ponto" />
                  <b>{repo.name}</b>
                  <span className="bv-mono">{failed ?? (repo.private ? `${repo.repo} · ${o.private}` : repo.repo)}</span>
                  <button className="bv-btn vidro bv-direita" disabled={running} onClick={() => void clone(repo.repo)}>
                    {running ? o.cloning : `${o.cloneTo} ${setup.clone_to}/`}
                  </button>
                </li>
              )
            })}
          </ul>
        </>
      )}
    </Frame>
  )
}
