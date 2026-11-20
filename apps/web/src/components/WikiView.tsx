import { useEffect, useState } from 'react'
import { type Bot, type WikiEntry, deleteWiki, fetchWiki, saveWiki, wikiPage } from '../api'
import { compact, t } from '../i18n'
import './wiki.css'

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error))

interface Draft {
  slug: string
  title: string
  body: string
  always: boolean
}

const EMPTY: Draft = { slug: '', title: '', body: '', always: false }

/** The team's wiki: pages every AI reads, each with what reading it cost this week. */
export function WikiView({ bots }: { bots: Bot[] }) {
  const [pages, setPages] = useState<WikiEntry[] | null>(null)
  const [draft, setDraft] = useState<Draft | null>(null)
  const [baseline, setBaseline] = useState<Draft | null>(null)
  const [loadingPage, setLoadingPage] = useState(false)
  const [query, setQuery] = useState('')
  const [saved, setSaved] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [discardTarget, setDiscardTarget] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    fetchWiki()
      .then((list) => setPages(list.pages))
      .catch((failure: unknown) => setError(messageOf(failure)))
  }, [])
  const dirty = draft !== null && JSON.stringify(draft) !== JSON.stringify(baseline)
  const edit = (next: Draft) => {
    setDraft(next)
    setSaved(false)
    setDiscardTarget(null)
  }
  const guard = (target: string, proceed: () => void) => {
    if (dirty && discardTarget !== target) {
      setDiscardTarget(target)
      return
    }
    setDiscardTarget(null)
    proceed()
  }
  const open = async (slug: string) => {
    setError(null)
    setSaved(false)
    setDeleting(false)
    setLoadingPage(true)
    try {
      const page = await wikiPage(slug)
      const loaded: Draft = { slug: page.slug, title: page.title, body: page.body, always: page.always }
      setDraft(loaded)
      setBaseline(loaded)
    } catch (failure: unknown) {
      setError(messageOf(failure))
    } finally {
      setLoadingPage(false)
    }
  }
  const startNew = () => {
    setError(null)
    setSaved(false)
    setDeleting(false)
    const fresh = { ...EMPTY }
    setDraft(fresh)
    setBaseline(fresh)
  }
  const save = async () => {
    if (!draft) return
    setError(null)
    try {
      const list = await saveWiki(draft)
      setPages(list.pages)
      const next = { ...draft, slug: list.saved ?? draft.slug }
      setDraft(next)
      setBaseline(next)
      setSaved(true)
      setDiscardTarget(null)
    } catch (failure: unknown) {
      setError(messageOf(failure))
    }
  }
  const remove = async () => {
    if (!draft?.slug) return
    if (!deleting) return setDeleting(true)
    try {
      setPages((await deleteWiki(draft.slug)).pages)
      setDraft(null)
      setBaseline(null)
      setDeleting(false)
    } catch (failure: unknown) {
      setError(messageOf(failure))
    }
  }
  const nameOf = (id: string) => bots.find((b) => b.id === id)?.persona?.name || bots.find((b) => b.id === id)?.name || id
  const current = pages?.find((p) => p.slug === draft?.slug)
  const needle = query.trim().toLowerCase()
  const visible = pages?.filter((p) => !needle || `${p.title} ${p.slug}`.toLowerCase().includes(needle)) ?? null
  return (
    <main className={`wiki ${draft ? 'editando' : ''}`}>
      <section className="cartao wiki-lista">
        <header className="acao">
          <button
            type="button"
            className={`cheio ${discardTarget === '__new__' ? 'confirmar' : ''}`}
            onBlur={() => discardTarget === '__new__' && setDiscardTarget(null)}
            onClick={() => guard('__new__', startNew)}
          >
            {discardTarget === '__new__' ? t.wikiUnsavedSure : t.wikiNew}
          </button>
        </header>
        {pages && pages.length > 0 && (
          <div className="busca-wrap wiki-busca">
            <input value={query} placeholder={t.wikiSearch} aria-label={t.wikiSearch} onChange={(event) => setQuery(event.target.value)} />
            {query && (
              <button type="button" className="campo-limpar" aria-label={t.clearSearch} title={t.clearSearch} onClick={() => setQuery('')}>
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            )}
          </div>
        )}
        {pages === null && !error && <p className="wiki-vazio">{t.wikiLoading}</p>}
        {pages && pages.length === 0 && <p className="wiki-vazio">{t.wikiEmpty}</p>}
        {visible && visible.length === 0 && pages && pages.length > 0 && <p className="wiki-vazio">{t.noMatches}</p>}
        <ul>
          {visible?.map((page) => {
            const confirming = discardTarget === page.slug
            return (
              <li key={page.slug}>
                <button
                  type="button"
                  className={`${draft?.slug === page.slug ? 'ativa' : ''} ${confirming ? 'confirmar' : ''}`}
                  onBlur={() => confirming && setDiscardTarget(null)}
                  onClick={() => (draft?.slug === page.slug ? undefined : guard(page.slug, () => void open(page.slug)))}
                >
                  <b>{confirming ? t.wikiUnsavedSure : page.title}</b>
                  <small>
                    <i className={`wiki-tipo ${page.always ? 'sempre' : ''}`} />
                    {page.reads > 0 ? (page.reads === 1 ? t.wikiReadsOne : t.wikiReads).replace('{n}', String(page.reads)).replace('{t}', compact(page.tokens)) : t.wikiNoReads}
                  </small>
                </button>
              </li>
            )
          })}
        </ul>
      </section>
      <section className="cartao wiki-pagina">
        {loadingPage ? (
          <p className="wiki-vazio">{t.wikiLoading}</p>
        ) : !draft ? (
          <p className="wiki-vazio">{t.wikiPick}</p>
        ) : (
          <form
            onSubmit={(event) => {
              event.preventDefault()
              void save()
            }}
          >
            <button
              type="button"
              className={`voltar wiki-voltar ${discardTarget === '__back__' ? 'confirmar' : ''}`}
              onBlur={() => discardTarget === '__back__' && setDiscardTarget(null)}
              onClick={() =>
                guard('__back__', () => {
                  setDraft(null)
                  setBaseline(null)
                })
              }
            >
              {discardTarget === '__back__' ? t.wikiUnsavedSure : t.wikiBack}
            </button>
            <label className="ajustes-campo">
              <span>{t.wikiTitleLabel}</span>
              <input value={draft.title} maxLength={80} onChange={(event) => edit({ ...draft, title: event.target.value })} />
            </label>
            <div className="wiki-tipos" role="radiogroup" aria-label={t.wikiTitle}>
              {[true, false].map((always) => (
                <button key={String(always)} type="button" role="radio" aria-checked={draft.always === always} className={draft.always === always ? 'ativo' : ''} onClick={() => edit({ ...draft, always })}>
                  <i className={`wiki-tipo ${always ? 'sempre' : ''}`} />
                  <span>
                    <b>{always ? t.wikiAlways : t.wikiMatching}</b>
                    <small>{always ? t.wikiAlwaysHint : t.wikiMatchingHint}</small>
                  </span>
                </button>
              ))}
            </div>
            <label className="ajustes-campo wiki-texto">
              <span>{`${t.wikiBodyLabel} · ${draft.body.length}/6000`}</span>
              <textarea value={draft.body} maxLength={6000} onChange={(event) => edit({ ...draft, body: event.target.value })} />
            </label>
            {current && current.reads > 0 && (
              <p className="wiki-leitores">
                {[
                  (current.reads === 1 ? t.wikiReadsOne : t.wikiReads).replace('{n}', String(current.reads)).replace('{t}', compact(current.tokens)),
                  t.wikiReadBy.replace('{who}', Object.entries(current.by).map(([id, n]) => `${nameOf(id)} (${n})`).join(', ')),
                ].join(' · ')}
              </p>
            )}
            <span className="acao">
              <button type="submit" className="cheio" disabled={!draft.title.trim() || !draft.body.trim()}>
                {t.wikiSave}
              </button>
              {dirty && <small className="wiki-alterada">{t.unsavedChanges}</small>}
              {saved && !dirty && <small className="wiki-salva">{t.wikiSaved}</small>}
              {draft.slug && (
                <button type="button" className={`vidro-btn wiki-apagar ${deleting ? 'confirmar' : ''}`} onBlur={() => setDeleting(false)} onClick={() => void remove()}>
                  {deleting ? t.wikiDeleteSure : t.wikiDelete}
                </button>
              )}
            </span>
          </form>
        )}
        {error && <p className="nota failed quebra">{error}</p>}
      </section>
    </main>
  )
}
