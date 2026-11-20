import { useEffect, useState } from 'react'
import { type Bot, type WikiEntry, deleteWiki, fetchWiki, saveWiki, wikiPage } from '../api'
import { t } from '../i18n'
import { type WikiDraft, WikiEditor } from './wiki/WikiEditor'
import { WikiIndex } from './wiki/WikiIndex'
import './wiki/wiki.css'

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error))
const EMPTY: WikiDraft = { slug: '', title: '', body: '', always: false }
/** What asks before unsaved changes are dropped, besides a page: a new page, or going back. */
const NEW = '__new__'
const BACK = '__back__'
/** Wide enough for the pages and a page side by side (the stylesheet's breakpoint). */
const SIDE_BY_SIDE = '(min-width: 961px)'

/** The team's wiki: pages every AI reads, by what they cost, and one page open to write. */
export function WikiView({ bots }: { bots: Bot[] }) {
  const [pages, setPages] = useState<WikiEntry[] | null>(null)
  const [draft, setDraft] = useState<WikiDraft | null>(null)
  const [baseline, setBaseline] = useState<WikiDraft | null>(null)
  const [loadingPage, setLoadingPage] = useState(false)
  const [query, setQuery] = useState('')
  const [saved, setSaved] = useState(false)
  const [confirming, setConfirming] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    fetchWiki()
      .then((list) => {
        setPages(list.pages)
        // Side by side, the wiki opens on a page (the ones every question carries first), not on a blank.
        const first = list.pages.find((p) => p.always) ?? list.pages[0]
        if (first && window.matchMedia(SIDE_BY_SIDE).matches) void open(first.slug)
      })
      .catch((failure: unknown) => setError(messageOf(failure)))
    // Once, when the page opens.
  }, [])
  const dirty = draft !== null && JSON.stringify(draft) !== JSON.stringify(baseline)
  const show = (next: WikiDraft | null) => {
    setDraft(next)
    setBaseline(next)
    setSaved(false)
    setConfirming(null)
  }
  // With unsaved changes, the first click asks; the second goes on.
  const guard = (target: string, proceed: () => void) => {
    if (dirty && confirming !== target) return setConfirming(target)
    setConfirming(null)
    proceed()
  }
  const open = async (slug: string) => {
    setError(null)
    setLoadingPage(true)
    try {
      const page = await wikiPage(slug)
      show({ slug: page.slug, title: page.title, body: page.body, always: page.always })
    } catch (failure: unknown) {
      setError(messageOf(failure))
    } finally {
      setLoadingPage(false)
    }
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
    } catch (failure: unknown) {
      setError(messageOf(failure))
    }
  }
  const remove = async () => {
    if (!draft?.slug) return
    try {
      setPages((await deleteWiki(draft.slug)).pages)
      show(null)
    } catch (failure: unknown) {
      setError(messageOf(failure))
    }
  }
  const startNew = () => guard(NEW, () => show({ ...EMPTY }))
  return (
    <main className={`wiki ${draft ? 'editando' : ''}`}>
      <WikiIndex
        pages={pages}
        query={query}
        onQuery={setQuery}
        open={draft?.slug ?? null}
        confirming={confirming}
        onPick={(slug) => slug !== draft?.slug && guard(slug, () => void open(slug))}
        onNew={startNew}
        onBlurConfirm={() => setConfirming(null)}
      />
      <section className="wiki-pagina">
        {loadingPage && <p className="wiki-vazio">{t.wikiLoading}</p>}
        {!loadingPage && !draft && (
          <div className="wiki-convite">
            {pages?.length === 0 && <img className="arte-vazia" src="/mascotes/vazio-wiki.webp" alt="" />}
            <p>{pages?.length === 0 ? t.wikiEmpty : t.wikiPick}</p>
          </div>
        )}
        {!loadingPage && draft && (
          <WikiEditor
            key={draft.slug || NEW}
            draft={draft}
            onChange={(next) => {
              setDraft(next)
              setSaved(false)
              setConfirming(null)
            }}
            week={pages?.find((p) => p.slug === draft.slug)}
            bots={bots}
            dirty={dirty}
            saved={saved}
            onSave={() => void save()}
            onDelete={() => void remove()}
            onBack={() => guard(BACK, () => show(null))}
            backConfirming={confirming === BACK}
            onBlurConfirm={() => setConfirming(null)}
          />
        )}
        {error && <p className="nota failed quebra">{error}</p>}
      </section>
    </main>
  )
}
