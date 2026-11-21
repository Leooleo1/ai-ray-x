import type { GitHubState, SetupState } from '../api'
import { t } from '../i18n'
import { Command, Frame } from './parts'
import { o } from './texts'

const PREVIEW = 8

interface StepGitHubProps {
  github: GitHubState | null
  setup: SetupState
  loading: boolean
  onCheck: () => void
  onBack: () => void
  onNext: () => void
}

export function StepGitHub({ github, setup, loading, onCheck, onBack, onNext }: StepGitHubProps) {
  const local = new Set(setup.projects.flatMap((p) => (p.github ? [p.github] : [])))
  const repos = github?.repos ?? []
  const here = repos.filter((r) => local.has(r.repo)).length
  return (
    <Frame
      actions={
        <>
          <button className="bv-btn link" onClick={onBack}>
            {o.back}
          </button>
          {!github?.logged_in && (
            <button className="bv-btn vidro" disabled={loading} onClick={onCheck}>
              {o.checkAgain}
            </button>
          )}
          <button className="bv-btn cheio bv-principal" disabled={loading} onClick={onNext}>
            {github?.logged_in ? o.continue : o.skip}
          </button>
        </>
      }
    >
      {!github && <p className="bv-espera">{o.asking}</p>}
      {github && !github.logged_in && (
        <div className="bv-instrucoes">
          <p>{github.installed ? o.githubLoggedOut : o.githubMissing}</p>
          {github.installed ? <Command label={o.loginWith} command={github.connect} /> : github.install ? (
            <Command label={o.installWith} command={github.install} />
          ) : (
            <a className="acao-texto" href="https://cli.github.com/" target="_blank" rel="noreferrer">{t.githubInstallHelp}</a>
          )}
        </div>
      )}
      {github?.logged_in && (
        <>
          <div className="bv-numero">
            <span>{repos.length}</span>
            <small>{`${o.repos} · ${here} ${o.onThisMachine} · ${o.githubAs} ${github.login}`}</small>
          </div>
          <ul className="bv-lista">
            {repos.slice(0, PREVIEW).map((repo) => (
              <li key={repo.repo} className="bv-repo">
                <i className={`ponto ${local.has(repo.repo) ? 'ok' : ''}`} />
                <b>{repo.name}</b>
                <span className="bv-mono">{repo.repo}</span>
                <span className="bv-mono bv-direita">
                  {[repo.private ? o.private : '', local.has(repo.repo) ? o.onThisMachine : o.onlyGitHub].filter(Boolean).join(' · ')}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
    </Frame>
  )
}
