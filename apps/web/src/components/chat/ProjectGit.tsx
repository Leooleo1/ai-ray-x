import { useEffect, useState } from 'react'
import { projectGitDiff, projectGitStatus, type GitChange, type ProjectGitDiff, type ProjectGitStatus } from '../../api'
import { t } from '../../i18n'
import type { ProjectReference } from './projectReference'
import './project-git.css'

type Load<T> = { value: T } | { error: string }
const areas = ['staged', 'unstaged', 'untracked'] as const

function loadError(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function Patch({ patch, openLine }: { patch: string; openLine?: (line: number) => void }) {
  let nextLine = 0
  const lines = patch.endsWith('\n') ? patch.slice(0, -1).split('\n') : patch.split('\n')
  return (
    <pre className="diff git-diff" tabIndex={0}>
      {lines.map((text, index) => {
        const header = /^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(text)
        if (text.startsWith('diff --git ')) nextLine = 0
        if (header) nextLine = Number(header[1])
        const added = nextLine > 0 && text.startsWith('+')
        const removed = nextLine > 0 && text.startsWith('-')
        const line = added || (nextLine > 0 && text.startsWith(' ')) ? nextLine++ : 0
        return (
          <span className={added ? 'mais' : removed ? 'menos' : header ? 'trecho' : ''} key={index}>
            {line > 0 && openLine && <button type="button" className="git-line" aria-label={t.gitOpenFile + ' · ' + line} onClick={() => openLine(line)}>{line}</button>}
            {text}
          </span>
        )
      })}
    </pre>
  )
}

export default function ProjectGit({ bot, close, openFile }: {
  bot: string
  close: () => void
  openFile: (reference: ProjectReference) => void
}) {
  const [revision, setRevision] = useState(0)
  const [status, setStatus] = useState<Load<ProjectGitStatus> | null>(null)
  const [selected, setSelected] = useState<GitChange | null>(null)
  const [diff, setDiff] = useState<Load<ProjectGitDiff> | null>(null)

  useEffect(() => {
    let current = true
    projectGitStatus(bot).then(
      (value) => { if (current) setStatus({ value }) },
      (error: unknown) => { if (current) setStatus({ error: loadError(error) }) },
    )
    return () => { current = false }
  }, [bot, revision])

  useEffect(() => {
    if (!selected) return
    let current = true
    projectGitDiff(bot, selected).then(
      (value) => { if (current) setDiff({ value }) },
      (error: unknown) => { if (current) setDiff({ error: loadError(error) }) },
    )
    return () => { current = false }
  }, [bot, selected])

  const summary = status && 'value' in status ? status.value : null
  const result = diff && 'value' in diff ? diff.value : null
  const labels = { staged: t.gitStaged, unstaged: t.gitUnstaged, untracked: t.gitUntracked }
  return (
    <section className="git-projeto" aria-label={t.gitTitle}>
      <header>
        <strong>{t.gitTitle}</strong>
        {summary?.repository && <code className="git-branch" title={summary.branch}>{summary.branch === '(detached)' ? t.gitDetached : summary.branch}</code>}
        <button type="button" className="vidro-btn" disabled={!status} onClick={() => {
          setSelected(null)
          setDiff(null)
          setStatus(null)
          setRevision(revision + 1)
        }}>{t.gitRefresh}</button>
        <button type="button" className="voltar" onClick={close}>{t.close}</button>
      </header>
      {!status && <p className="nota" role="status">{t.gitLoading}</p>}
      {status && 'error' in status && <p className="nota failed quebra" role="alert">{status.error}</p>}
      {summary && !summary.repository && <p className="nota">{t.gitNoRepo}</p>}
      {summary?.repository && summary.changes.length === 0 && <p className="nota">{t.gitClean}</p>}
      {summary?.repository && summary.changes.length > 0 && (
        <div className="git-area">
          <aside>
            {areas.map((area) => {
              const changes = summary.changes.filter((change) => change.area === area)
              return changes.length > 0 && (
                <section key={area}>
                  <h3>{labels[area]} <small>{changes.length}</small></h3>
                  <ul>
                    {changes.map((change) => (
                      <li key={change.path}>
                        <button type="button" aria-pressed={selected === change} onClick={() => {
                          if (selected === change) return
                          setDiff(null)
                          setSelected(change)
                        }}>
                          <small>{change.code}</small><code title={change.path}>{change.path}</code>
                        </button>
                      </li>
                    ))}
                  </ul>
                </section>
              )
            })}
          </aside>
          <section className="git-review">
            {selected ? (
              <>
                <header>
                  <code>{selected.old_path ? selected.old_path + ' → ' : ''}{selected.path}</code>
                  {result?.editable && <button type="button" className="vidro-btn" onClick={() => openFile({ path: selected.path })}>{t.gitOpenFile}</button>}
                </header>
                {!diff && <p className="nota" role="status">{t.gitLoading}</p>}
                {diff && 'error' in diff && <p className="nota failed quebra" role="alert">{diff.error}</p>}
                {result?.patch && <Patch patch={result.patch} openLine={result.editable && selected.area !== 'staged' ? (line) => openFile({ path: selected.path, line }) : undefined} />}
                {result && !result.patch && <p className="nota">{t.gitNoDiff}</p>}
                {result?.limited && <p className="nota">{t.gitLimited}</p>}
              </>
            ) : <p className="nota">{t.gitSelect}</p>}
          </section>
        </div>
      )}
    </section>
  )
}
