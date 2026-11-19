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
  const [saved, setSaved] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    fetchWiki()
      .then((list) => setPages(list.pages))
      .catch((failure: unknown) => setError(messageOf(failure)))
  }, [])
  const open = async (slug: string) => {
    setError(null)
    setSaved(false)
    setDeleting(false)
    try {
      const page = await wikiPage(slug)
      setDraft({ slug: page.slug, title: page.title, body: page.body, always: page.always })
    } catch (failure: unknown) {
      setError(messageOf(failure))
    }
  }
  const save = async () => {
    if (!draft) return
    setError(null)
    try {
      const list = await saveWiki(draft)
      setPages(list.pages)
      setDraft({ ...draft, slug: list.saved ?? draft.slug })
      setSaved(true)
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
    } catch (failure: unknown) {
      setError(messageOf(failure))
    }
  }
  const nameOf = (id: string) => bots.find((b) => b.id === id)?.persona?.name || bots.find((b) => b.id === id)?.name || id
  const current = pages?.find((p) => p.slug === draft?.slug)
  return (
    <main className={`wiki ${draft ? 'editando' : ''}`}>
      <section className="cartao wiki-lista">
        <header className="acao">
          <button type="button" className="cheio" onClick={() => setDraft({ ...EMPTY })}>
            {t.wikiNew}
          </button>
        </header>
        {pages && pages.length === 0 && <p className="wiki-vazio">{t.wikiEmpty}</p>}
        <ul>
          {pages?.map((page) => (
            <li key={page.slug}>
              <button type="button" className={draft?.slug === page.slug ? 'ativa' : ''} onClick={() => void open(page.slug)}>
                <b>{page.title}</b>
                <small>
                  <i className={`wiki-tipo ${page.always ? 'sempre' : ''}`} />
                  {page.reads > 0 ? (page.reads === 1 ? t.wikiReadsOne : t.wikiReads).replace('{n}', String(page.reads)).replace('{t}', compact(page.tokens)) : t.wikiNoReads}
                </small>
              </button>
            </li>
          ))}
        </ul>
      </section>
      <section className="cartao wiki-pagina">
        {!draft ? (
          <p className="wiki-vazio">{t.wikiPick}</p>
        ) : (
          <form
            onSubmit={(event) => {
              event.preventDefault()
              void save()
            }}
          >
            <button type="button" className="voltar wiki-voltar" onClick={() => setDraft(null)}>
              {t.wikiBack}
            </button>
            <label className="ajustes-campo">
              <span>{t.wikiTitleLabel}</span>
              <input value={draft.title} maxLength={80} onChange={(event) => setDraft({ ...draft, title: event.target.value })} />
            </label>
            <div className="wiki-tipos" role="radiogroup">
              {[true, false].map((always) => (
                <button key={String(always)} type="button" role="radio" aria-checked={draft.always === always} className={draft.always === always ? 'ativo' : ''} onClick={() => setDraft({ ...draft, always })}>
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
              <textarea value={draft.body} maxLength={6000} onChange={(event) => setDraft({ ...draft, body: event.target.value })} />
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
              {saved && <small className="wiki-salva">{t.wikiSaved}</small>}
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
