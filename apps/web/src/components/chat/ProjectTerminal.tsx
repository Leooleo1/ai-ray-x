import { useEffect, useRef, useState } from 'react'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { type TerminalKind, type TerminalOutput, terminalAction } from '../../api'
import { t } from '../../i18n'
import { type ProjectReference, testLocations } from './projectReference'
import './project-terminal.css'

interface View {
  instance: Terminal
  dispose: boolean
  cursor: number
  session: number | undefined
  revision: number
  decoder: TextDecoder
  output: string
  starting: boolean
}

export default function ProjectTerminal({ bot, root, visible, close, openFile, addToDraft }: {
  bot: string
  root: string
  visible: boolean
  close: () => void
  openFile: (file: ProjectReference) => void
  addToDraft: (text: string) => string | null
}) {
  const [kind, setKind] = useState<TerminalKind>('shell')
  const [status, setStatus] = useState<TerminalOutput | null>(null)
  const [checking, setChecking] = useState(true)
  const [working, setWorking] = useState(false)
  const [error, setError] = useState('')
  const [command, setCommand] = useState('')
  const [suggestions, setSuggestions] = useState<string[]>([])
  const [locations, setLocations] = useState<ProjectReference[]>([])
  const [hasSelection, setHasSelection] = useState(false)
  const [draftError, setDraftError] = useState<string | null>(null)
  const host = useRef<HTMLDivElement>(null)
  const view = useRef<View | null>(null)
  const running = status !== null && status.code === null

  function consume(current: View, next: TerminalOutput | null) {
    if (current.dispose) return
    if (!next) {
      current.session = undefined
      current.cursor = 0
      setStatus(null)
      return
    }
    const changed = current.session !== next.session
    if (next.reset || changed) {
      current.instance.reset()
      current.decoder = new TextDecoder()
      current.output = ''
      current.cursor = 0
    }
    const bytes = Uint8Array.from(atob(next.bytes), (byte) => byte.charCodeAt(0))
    const offset = Math.max(0, current.cursor - (next.cursor - bytes.length))
    if (bytes.length > offset) {
      const added = bytes.subarray(offset)
      current.instance.write(added)
      if (kind === 'test') {
        current.output = (current.output + current.decoder.decode(added, { stream: true })).slice(-256 * 1024)
        setLocations(testLocations(current.output, root))
      }
    } else if (changed) {
      setLocations([])
    }
    current.cursor = Math.max(current.cursor, next.cursor)
    current.session = next.session
    if (changed && next.code === null) {
      void terminalAction(bot, kind, 'resize', { session: next.session, rows: current.instance.rows, cols: current.instance.cols }).catch(() => undefined)
    }
    const { bytes: _bytes, ...summary } = next
    setStatus({ ...summary, bytes: '' })
  }

  useEffect(() => {
    if (!visible || kind !== 'test') return undefined
    let active = true
    terminalAction<{ commands: string[] }>(bot, 'test', 'commands').then(({ commands }) => {
      if (!active) return
      setSuggestions(commands)
      setCommand((current) => current || commands[0] || '')
    }).catch(() => undefined)
    return () => { active = false }
  }, [bot, visible, kind])

  useEffect(() => {
    if (!visible || !host.current) return undefined
    setError('')
    setChecking(true)
    setStatus(null)
    setLocations([])
    setWorking(false)
    setHasSelection(false)
    setDraftError(null)
    const instance = new Terminal({
      fontFamily: 'Geist Mono Variable, monospace',
      fontSize: 13,
      scrollback: 2500,
      cursorBlink: kind === 'shell',
      disableStdin: kind === 'test',
      theme: { background: '#0d0d11', foreground: '#e6e6ec' },
      linkHandler: { activate: () => undefined },
    })
    const fit = new FitAddon()
    instance.loadAddon(fit)
    instance.open(host.current)
    const current: View = { instance, dispose: false, cursor: 0, session: undefined, revision: 0, decoder: new TextDecoder(), output: '', starting: false }
    view.current = current
    let timer: number | undefined
    const poll = async () => {
      const revision = current.revision
      let delay = 300
      if (!current.starting) {
        try {
          const next = await terminalAction(bot, kind, 'poll', { session: current.session, cursor: current.cursor })
          if (!current.dispose && revision === current.revision) {
            consume(current, next)
            setChecking(false)
            if (next?.more) delay = 0
            else if (!next || next.code !== null) delay = 1000
          }
        } catch {
          if (!current.dispose && revision === current.revision) {
            setChecking(false)
            setError(t.terminalError)
          }
          delay = 1000
        }
      }
      if (!current.dispose) timer = window.setTimeout(() => void poll(), delay)
    }
    let inputs = Promise.resolve()
    const input = instance.onData((data) => {
      const session = current.session
      if (kind !== 'shell' || session === undefined || current.starting) return
      inputs = inputs.then(async () => {
        for (const chunk of data.match(/[\s\S]{1,4096}/gu) ?? []) {
          if (current.dispose || session !== current.session) break
          await terminalAction(bot, kind, 'input', { session, data: chunk })
        }
      }).catch(() => { if (!current.dispose) setError(t.terminalError) })
    })
    const selection = instance.onSelectionChange(() => {
      setHasSelection(Boolean(instance.getSelection().trim()))
    })
    const resize = instance.onResize(({ rows, cols }) => {
      if (current.session !== undefined) void terminalAction(bot, kind, 'resize', { session: current.session, rows, cols }).catch(() => undefined)
    })
    const observer = new ResizeObserver(() => fit.fit())
    observer.observe(host.current)
    fit.fit()
    void poll()
    return () => {
      current.dispose = true
      window.clearTimeout(timer)
      observer.disconnect()
      input.dispose()
      selection.dispose()
      resize.dispose()
      instance.dispose()
      view.current = null
    }
  }, [bot, kind, root, visible])

  const start = async () => {
    const current = view.current
    if (!current || current.starting || working || running || (kind === 'test' && !command.trim())) return
    current.starting = true
    current.revision += 1
    setWorking(true)
    setError('')
    try {
      const next = await terminalAction(bot, kind, 'start', { command, rows: current.instance.rows, cols: current.instance.cols })
      consume(current, next)
      if (kind === 'shell' && !current.dispose) current.instance.focus()
    } catch {
      if (!current.dispose) setError(t.terminalError)
    } finally {
      current.starting = false
      if (!current.dispose) setWorking(false)
    }
  }

  const stop = async () => {
    if (status?.session === undefined || working) return
    setWorking(true)
    const current = view.current
    try {
      await terminalAction(bot, kind, 'stop', { session: status.session })
    } catch {
      if (current && !current.dispose) setError(t.terminalError)
    } finally {
      if (current && !current.dispose) setWorking(false)
    }
  }

  const addSelection = () => {
    const selected = view.current?.instance.getSelection().replace(/\r\n/g, '\n')
    if (!selected?.trim()) return
    setDraftError(addToDraft(selected))
  }

  if (!visible) return null
  const caption = checking ? t.filesLoading : !status ? t.terminalReady
    : status.code === null ? status.stopped ? t.terminalStopping : t.terminalRunning
      : status.stopped ? t.terminalStopped : status.code === 0 ? t.terminalSuccess : t.terminalExit.replace('{code}', String(status.code))
  return (
    <section className="terminal-projeto" aria-label={t.terminalTitle}>
      <header>
        <div className="terminal-modos">
          <button type="button" className="vidro-btn" aria-pressed={kind === 'shell'} onClick={() => setKind('shell')}>{t.terminalTitle}</button>
          <button type="button" className="vidro-btn" aria-pressed={kind === 'test'} onClick={() => setKind('test')}>{t.terminalTests}</button>
        </div>
        <button type="button" className="voltar" onClick={close}>{t.close}</button>
      </header>
      <div className="terminal-controles">
        {kind === 'test' && (
          <form onSubmit={(event) => { event.preventDefault(); void start() }}>
            <input aria-label={t.terminalCommand} placeholder={t.terminalCommand} value={command} maxLength={1024}
              list="terminal-test-commands" onChange={(event) => setCommand(event.target.value)} />
            <datalist id="terminal-test-commands">{suggestions.map((item) => <option key={item} value={item} />)}</datalist>
            <button type="submit" className="vidro-btn" disabled={checking || working || running || !command.trim()}>{t.terminalRunTests}</button>
          </form>
        )}
        {kind === 'shell' && !running && <button type="button" className="vidro-btn" disabled={checking || working} onClick={() => void start()}>{t.terminalStart}</button>}
        {running && <button type="button" className="vidro-btn" disabled={working || status.stopped} onClick={() => void stop()}>{t.stop}</button>}
        <button type="button" className="vidro-btn" disabled={!hasSelection} title={t.terminalSelectOutput} onClick={addSelection}>
          {t.terminalAddToDraft}
        </button>
        <small role="status">{caption}</small>
        {kind === 'test' && status?.command && <code title={status.command}>{status.command}</code>}
        {error && <small role="alert">{error}</small>}
        {draftError && <small role="alert">{draftError}</small>}
      </div>
      <div className="terminal-tela" ref={host} />
      {kind === 'test' && locations.length > 0 && (
        <div className="terminal-locais">
          <strong>{t.terminalLocations}</strong>
          {locations.map((file) => {
            const label = file.path + ':' + file.line
            return <button type="button" key={label} title={label} className="acao-texto" onClick={() => openFile(file)}>{label}</button>
          })}
        </div>
      )}
    </section>
  )
}
