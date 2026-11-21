import type { Ref } from 'react'
import { IS_REMOTE, type CliInstall, type GitHubState } from '../../api'
import { t } from '../../i18n'
import { Command } from '../../onboarding/parts'

interface GitHubTileProps {
  tileRef: Ref<HTMLElement>
  github: GitHubState | null
  checking: boolean
  error: string | null
  action: 'install' | 'connect' | null
  install?: CliInstall['state']
  code: string | null
  onAction: () => void
  onCheck: () => void
}

export function GitHubTile({ tileRef, github, checking, error, action, install, code, onAction, onCheck }: GitHubTileProps) {
  const connected = github?.logged_in === true
  const installing = action === 'install' || install === 'running'
  const busy = checking || installing || action === 'connect'
  const status = installing ? t.installing : action === 'connect' ? t.githubStarting : checking ? t.probing
    : connected ? `${t.githubAs} ${github.login}` : code ? t.githubWaiting : !github ? t.githubStatusUnavailable
      : github.installed ? t.githubSignedOut : t.githubNoGh
  const issue = error ?? (github && !github.installed && !busy
    ? install === 'failed' ? t.installStopped : install === 'done' ? t.githubInstallReopen : null
    : null)
  const canStart = github && !connected && !IS_REMOTE && !code && (github.installed || github.can_install)

  return (
    <article className="cfg-ia cfg-github" id="cfg-github" ref={tileRef} tabIndex={-1} aria-labelledby="cfg-github-title">
      <header>
        <span className="cfg-monograma">
          <img src="/marcas/github.svg" width={17} height={17} alt="" />
        </span>
        <span className="cfg-nome">
          <b id="cfg-github-title">{t.githubTitle}</b>
          <small>GitHub CLI</small>
        </span>
        <button type="button" className="cfg-btn vidro pequeno" disabled={busy} onClick={onCheck}>
          {t.checkAgain}
        </button>
      </header>
      <p className={`cfg-estado ${connected ? 'ok' : ''}`} role="status">
        <i className={`ponto ${connected ? 'ok' : ''}`} />
        <span>{status}</span>
      </p>
      {connected && (
        <p className="cfg-numero">
          <b>{github.repos.length}</b>
          <small>{t.githubRepos}</small>
        </p>
      )}
      {(canStart || issue || code || (github && !connected)) && (
        <footer className="cfg-comandos">
          {issue && <p className="cfg-faz falhou" role="alert">{issue}</p>}
          {canStart && (
            <button type="button" className="cfg-btn cheio pequeno" disabled={busy} onClick={onAction}>
              {github.installed ? t.nextGithub : t.githubInstall}
            </button>
          )}
          {code && (
            <>
              <p className="cfg-faz">{t.githubDeviceCode}</p>
              <Command command={code} />
              <a className="acao-texto" href="https://github.com/login/device" target="_blank" rel="noreferrer">{t.githubOpenDevice}</a>
            </>
          )}
          {github && !connected && !code && (IS_REMOTE ? <p className="cfg-faz">{t.githubDesktopOnly}</p> : !busy && (
            github.installed ? (
              <details>
                <summary>{t.githubTerminal}</summary>
                <Command label={t.signInLabel} command={github.connect} />
              </details>
            ) : (!github.can_install || issue) && (
              <a className="acao-texto" href="https://cli.github.com/" target="_blank" rel="noreferrer">{t.githubInstallHelp}</a>
            )
          ))}
        </footer>
      )}
    </article>
  )
}
