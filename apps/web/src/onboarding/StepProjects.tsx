import { useState } from 'react'
import { type GitHubState, type LocalProject, type SetupState, cloneRepo } from '../api'
import { compact } from '../i18n'
import { Frame } from './parts'
import { o } from './texts'

const VISIBLE = 8
/** Bots made in one go; more can be added later from the Bots page. */
export const MAX_PICK = 6

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
  const needle = query.trim().toLowerCase()
  const matches = setup.projects.filter((p) => !needle || `${p.project} ${p.github ?? ''}`.toLowerCase().includes(needle))
  // Checked projects stay in view even when the search would hide them.
  const shown = needle ? matches : [...setup.projects.filter((p) => selected.includes(p.project)), ...matches.filter((p) => !selected.includes(p.project))].slice(0, Math.max(VISIBLE, selected.length))
  const hidden = needle ? 0 : setup.projects.length - shown.length
  const local = new Set(setup.projects.flatMap((p) => (p.github ? [p.github] : [])))
  const remote = (github?.repos ?? []).filter((r) => !local.has(r.repo) && (!needle || r.repo.toLowerCase().includes(needle)))

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
      <input className="bv-busca" value={query} onChange={(event) => setQuery(event.target.value)} placeholder={o.search} aria-label={o.search} />
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
