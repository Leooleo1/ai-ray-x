import { useEffect, useEffectEvent, useRef, useState } from 'react'
import { type ProjectEntry, type ProjectMatch, projectFiles, readProjectFile, saveProjectFile, searchProjectFiles } from '../../api'
import { t } from '../../i18n'
import type { ProjectReference } from './projectReference'
import './project-files.css'

interface FileTab {
  saved: string
  text: string
  version: string
  crlf: boolean
}

const drafts = new Map<string, FileTab>()
let watchingDrafts = false

function trackDraft(key: string, file: FileTab) {
  if (!watchingDrafts) {
    watchingDrafts = true
    window.addEventListener('beforeunload', (event) => {
      if (!drafts.size) return
      event.preventDefault()
      event.returnValue = ''
    })
  }
  if (file.text === file.saved) drafts.delete(key)
  else drafts.set(key, file)
}

function FileTree({ bot, path, refresh, open }: { bot: string; path: string; refresh: number; open: (reference: ProjectReference) => void }) {
  const [entries, setEntries] = useState<ProjectEntry[] | null>(null)
  const [failed, setFailed] = useState(false)
  const [expanded, setExpanded] = useState<string[]>([])
  useEffect(() => {
    let active = true
    setFailed(false)
    projectFiles(bot, path).then((next) => { if (active) setEntries(next.entries) })
      .catch(() => { if (active) setFailed(true) })
    return () => { active = false }
  }, [bot, path, refresh])
  if (failed) return <p className="arquivos-nota">{t.filesUnavailable}</p>
  if (!entries) return <p className="arquivos-nota">{t.filesLoading}</p>
  if (!entries.length) return <p className="arquivos-nota">{t.filesEmpty}</p>
  return (
    <ul className="arquivos-arvore">
      {entries.map((entry) => {
        const isOpen = expanded.includes(entry.path)
        return (
          <li key={entry.path}>
            <button type="button" title={entry.path} aria-expanded={entry.directory ? isOpen : undefined}
              onClick={() => entry.directory
                ? setExpanded((before) => isOpen ? before.filter((item) => item !== entry.path) : [...before, entry.path])
                : open({ path: entry.path })}>
              {entry.directory && <span aria-hidden="true">{isOpen ? '⌄' : '›'} </span>}{entry.name}
            </button>
            {entry.directory && isOpen && <FileTree bot={bot} path={entry.path} refresh={refresh} open={open} />}
          </li>
        )
      })}
    </ul>
  )
}

export function ProjectFiles({ bot, visible, request, close, addToDraft }: {
  bot: string
  visible: boolean
  request: ProjectReference | null
  close: () => void
  addToDraft: (reference: string, text: string) => string | null
}) {
  const [refresh, setRefresh] = useState(0)
  const [query, setQuery] = useState('')
  const [result, setResult] = useState<{ matches: ProjectMatch[]; limited: boolean } | null>(null)
  const [searchFailed, setSearchFailed] = useState(false)
  const [selection, setSelection] = useState<ProjectReference | null>(null)
  const [tabs, setTabs] = useState<Record<string, FileTab>>({})
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [saving, setSaving] = useState(false)
  const [conflict, setConflict] = useState(false)
  const [hasExcerpt, setHasExcerpt] = useState(false)
  const [excerptError, setExcerptError] = useState<string | null>(null)
  const editor = useRef<HTMLTextAreaElement>(null)
  const numbers = useRef<HTMLDivElement>(null)
  const openSequence = useRef(0)
  const path = selection?.path ?? ''
  const file = Object.hasOwn(tabs, path) ? tabs[path] : undefined
  const dirty = file && file.saved !== file.text
  const draftKey = (name: string) => JSON.stringify([bot, name])

  useEffect(() => {
    if (!query.trim() || !visible) return undefined
    let active = true
    const timer = window.setTimeout(() => {
      searchProjectFiles(bot, query).then((next) => { if (active) setResult(next) })
        .catch(() => { if (active) setSearchFailed(true) })
    }, 220)
    return () => {
      active = false
      window.clearTimeout(timer)
    }
  }, [bot, query, visible, refresh])

  const open = async (reference: ProjectReference, reload = false) => {
    if (saving) return
    const sequence = ++openSequence.current
    const next = reference.path
    setSelection(reference)
    setHasExcerpt(false)
    setExcerptError(null)
    setError('')
    setConflict(false)
    setLoading(false)
    const existing = Object.hasOwn(tabs, next) ? tabs[next] : drafts.get(draftKey(next))
    if (existing && !reload) {
      setTabs((before) => Object.hasOwn(before, next) ? before : { ...before, [next]: existing })
      return
    }
    setLoading(true)
    try {
      const { contents, version } = await readProjectFile(bot, next)
      if (sequence !== openSequence.current) return
      const text = contents.replaceAll('\r\n', '\n')
      const tab = { saved: text, text, version, crlf: contents.includes('\r\n') && !contents.replaceAll('\r\n', '').includes('\n') }
      setTabs((before) => ({ ...before, [next]: tab }))
      if (reload) drafts.delete(draftKey(next))
    } catch {
      if (sequence === openSequence.current) setError(t.fileReadFailed)
    } finally {
      if (sequence === openSequence.current) setLoading(false)
    }
  }
  const openFromChat = useEffectEvent((reference: ProjectReference) => void open(reference))
  useEffect(() => { if (visible && request) openFromChat(request) }, [visible, request])

  useEffect(() => {
    const input = editor.current
    if (!input || !selection?.line || loading) return
    const lines = input.value.split('\n')
    const line = Math.min(lines.length, selection.line)
    const offset = lines.slice(0, line - 1).reduce((total, row) => total + row.length + 1, 0)
    input.focus()
    input.setSelectionRange(offset, offset + lines[line - 1].length)
    setHasExcerpt(Boolean(lines[line - 1].trim()))
    input.scrollTop = Math.max(0, (line - 3) * 21)
    if (numbers.current) numbers.current.scrollTop = input.scrollTop
  }, [selection, loading])

  const addExcerpt = () => {
    const input = editor.current
    if (!input) return
    const { value, selectionStart, selectionEnd } = input
    const text = value.slice(selectionStart, selectionEnd)
    if (!text.trim()) return
    const first = value.slice(0, selectionStart).split('\n').length
    const last = first + text.slice(0, -1).split('\n').length - 1
    const reference = path + '#L' + first + (last > first ? '-L' + last : '')
    setExcerptError(addToDraft(reference, text))
  }

  const edit = (text: string) => {
    if (!file) return
    const next = { ...file, text }
    trackDraft(draftKey(path), next)
    setTabs((before) => ({ ...before, [path]: next }))
  }
  const save = async () => {
    if (!file || !dirty || saving || conflict) return
    setSaving(true)
    setError('')
    try {
      const contents = file.crlf ? file.text.replaceAll('\n', '\r\n') : file.text
      const response = await saveProjectFile(bot, path, contents, file.version)
      if (response.conflict) { setConflict(true); return }
      if (!response.version) throw new Error(t.fileSaveFailed)
      const next = { ...file, saved: file.text, version: response.version }
      if (drafts.get(draftKey(path)) === file) drafts.delete(draftKey(path))
      setTabs((before) => ({ ...before, [path]: next }))
      setConflict(false)
    } catch {
      setError(t.fileSaveFailed)
    } finally {
      setSaving(false)
    }
  }
  const closeTab = (name: string) => {
    if (tabs[name].text !== tabs[name].saved && !window.confirm(t.fileDiscard)) return
    drafts.delete(draftKey(name))
    const next = { ...tabs }
    delete next[name]
    setTabs(next)
    if (path === name) {
      const other = Object.keys(next).at(-1)
      if (other) void open({ path: other })
      else { setSelection(null); setError(''); setConflict(false) }
    }
  }

  if (!visible) return null
  return (
    <section className="arquivos-projeto" aria-label={t.filesTitle}
      onKeyDown={(event) => {
        if (event.key === 'Escape') close()
        if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
          event.preventDefault()
          void save()
        }
      }}>
      <header><strong>{t.filesTitle}</strong><button type="button" className="voltar" onClick={close}>{t.close}</button></header>
      <div className="arquivos-area">
        <aside className="arquivos-lateral">
          <div className="arquivos-busca">
            <input type="search" maxLength={64} aria-label={t.filesSearch} placeholder={t.filesSearch} value={query}
              onChange={(event) => { setQuery(event.target.value); setResult(null); setSearchFailed(false) }} />
            <button type="button" className="acao-texto" title={t.filesRefresh} onClick={() => setRefresh((value) => value + 1)}>{t.filesRefresh}</button>
          </div>
          {query.trim() ? searchFailed ? <p className="arquivos-nota">{t.filesUnavailable}</p> : !result ? <p className="arquivos-nota">{t.filesLoading}</p> : (
            <ul className="arquivos-resultados">
              {result.matches.map((match) => (
                <li key={JSON.stringify([match.path, match.line])}>
                  <button type="button" onClick={() => void open(match)}>
                    <span>{match.path}{match.line ? ':' + match.line : ''}</span>
                    {match.excerpt && <small>{match.excerpt}</small>}
                  </button>
                </li>
              ))}
              {!result.matches.length && <li className="arquivos-nota">{t.filesNoMatches}</li>}
              {result.limited && <li className="arquivos-nota">{t.filesLimited}</li>}
            </ul>
          ) : <FileTree bot={bot} path="" refresh={refresh} open={(reference) => void open(reference)} />}
        </aside>
        <div className="arquivos-edicao">
          <div className="arquivos-abas">
            {Object.entries(tabs).map(([name, tab]) => (
              <div key={name} className={path === name ? 'ativa' : ''}>
                <button type="button" title={name} aria-pressed={path === name} onClick={() => void open({ path: name })}>
                  {name.split('/').at(-1)}{tab.text !== tab.saved ? ' •' : ''}
                </button>
                <button type="button" disabled={saving} aria-label={t.fileClose.replace('{name}', name)} onClick={() => closeTab(name)}>×</button>
              </div>
            ))}
          </div>
          {path && (
            <div className="arquivos-barra">
              <span title={path}>{path}</span>
              {file && (
                <div className="arquivos-acoes">
                  <button type="button" className="vidro-btn" disabled={!hasExcerpt || loading} title={t.fileSelectExcerpt} onClick={addExcerpt}>
                    {t.fileAddToDraft}
                  </button>
                  <button type="button" className="vidro-btn" disabled={!dirty || loading || saving || conflict} onClick={() => void save()}>{saving ? t.fileSaving : t.fileSave}</button>
                </div>
              )}
            </div>
          )}
          {error && <p className="arquivos-nota" role="alert">{error}</p>}
          {path && excerptError && <p className="arquivos-nota" role="alert">{excerptError}</p>}
          {conflict && (
            <div className="arquivos-nota" role="alert">
              {t.fileConflict} <button type="button" className="acao-texto" onClick={() => { if (window.confirm(t.fileDiscard)) void open({ path }, true) }}>{t.fileReload}</button>
            </div>
          )}
          {loading ? <p className="arquivos-nota">{t.filesLoading}</p> : file ? (
            <div className="arquivos-editor">
              <div ref={numbers} className="arquivos-linhas" aria-hidden="true">
                {Array.from({ length: file.text.split('\n').length }, (_, index) => index + 1).join('\n')}
              </div>
              <textarea key={path} ref={editor} aria-label={t.fileEditor} wrap="off" spellCheck={false} readOnly={saving}
                value={file.text} onChange={(event) => edit(event.target.value)}
                onSelect={(event) => {
                  const { value, selectionStart, selectionEnd } = event.currentTarget
                  setHasExcerpt(Boolean(value.slice(selectionStart, selectionEnd).trim()))
                }}
                onScroll={(event) => { if (numbers.current) numbers.current.scrollTop = event.currentTarget.scrollTop }} />
            </div>
          ) : !path && <p className="arquivos-nota">{t.fileSelect}</p>}
        </div>
      </div>
    </section>
  )
}
