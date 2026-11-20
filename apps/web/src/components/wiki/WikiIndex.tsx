import { Skeleton } from '../Skeleton'
import type { WikiEntry } from '../../api'
import { compact, t } from '../../i18n'
import { CostBar } from './CostBar'

interface WikiIndexProps {
  pages: WikiEntry[] | null
  query: string
  onQuery: (query: string) => void
  /** The page open now, by its slug. */
  open: string | null
  /** A page (or "new") that asks before its unsaved changes are dropped. */
  confirming: string | null
  onPick: (slug: string) => void
  onNew: () => void
  onBlurConfirm: () => void
}

const NEW = '__new__'

/** A page's week in a few characters: "48× · 768 tokens", or that nobody read it. */
function weekOf(page: WikiEntry): string {
  if (page.reads === 0) return t.wikiNoReads
  return t.wikiWeekShort.replace('{n}', String(page.reads)).replace('{t}', compact(page.tokens))
}

/** The pages, split by what they cost: those that go with every question, and those that go when it matches. */
export function WikiIndex({ pages, query, onQuery, open, confirming, onPick, onNew, onBlurConfirm }: WikiIndexProps) {
  const needle = query.trim().toLowerCase()
  const visible = pages?.filter((p) => !needle || `${p.title} ${p.slug}`.toLowerCase().includes(needle)) ?? []
  const most = Math.max(0, ...(pages ?? []).map((p) => p.tokens))
  const reads = (pages ?? []).reduce((sum, p) => sum + p.reads, 0)
  const tokens = (pages ?? []).reduce((sum, p) => sum + p.tokens, 0)
  const groups = [
    { always: true, title: t.wikiAlways, items: visible.filter((p) => p.always) },
    { always: false, title: t.wikiMatching, items: visible.filter((p) => !p.always) },
  ].filter((group) => group.items.length > 0)
  return (
    <section className="wiki-indice" aria-label={t.wikiTitle}>
      <header className="wiki-indice-topo">
        <div className="busca-wrap wiki-busca">
          <input value={query} placeholder={t.wikiSearch} aria-label={t.wikiSearch} onChange={(event) => onQuery(event.target.value)} />
          {query && (
            <button type="button" className="campo-limpar" aria-label={t.clearSearch} title={t.clearSearch} onClick={() => onQuery('')}>
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>
        <button type="button" className={`wiki-nova ${confirming === NEW ? 'confirmar' : ''}`} onBlur={onBlurConfirm} onClick={onNew}>
          {confirming === NEW ? t.wikiUnsavedSure : t.wikiNew}
        </button>
      </header>
      <div className="wiki-grupos">
        {pages === null && <Skeleton shape="rows" rows={5} />}
        {pages?.length === 0 && <p className="wiki-vazio">{t.wikiEmpty}</p>}
        {pages && pages.length > 0 && visible.length === 0 && <p className="wiki-vazio">{t.noMatches}</p>}
        {groups.map((group) => (
          <div key={String(group.always)} className="wiki-grupo">
            <h3>
              {group.title}
              <small>{group.items.length}</small>
            </h3>
            <ul>
              {group.items.map((page) => (
                <li key={page.slug}>
                  <button type="button" className={`${open === page.slug ? 'ativa' : ''} ${confirming === page.slug ? 'confirmar' : ''}`} onBlur={onBlurConfirm} onClick={() => onPick(page.slug)}>
                    <b>{confirming === page.slug ? t.wikiUnsavedSure : page.title}</b>
                    <CostBar tokens={page.tokens} most={most} />
                    <small>{weekOf(page)}</small>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
      {pages && pages.length > 0 && <footer className="wiki-semana">{t.wikiWeekTotal.replace('{n}', String(reads)).replace('{t}', compact(tokens))}</footer>}
    </section>
  )
}
