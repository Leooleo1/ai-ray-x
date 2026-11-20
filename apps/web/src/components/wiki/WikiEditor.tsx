import { useState } from 'react'
import type { Bot, WikiEntry } from '../../api'
import { faceForBot } from '../../avatars'
import { compact, t } from '../../i18n'
import { Avatar } from '../Avatar'

export interface WikiDraft {
  slug: string
  title: string
  body: string
  always: boolean
}

export const TITLE_MAX = 80
export const BODY_MAX = 6000
/** Readers named in the footer; the rest (and AIs removed since) are summed up. */
const READERS_SHOWN = 4

interface WikiEditorProps {
  draft: WikiDraft
  onChange: (draft: WikiDraft) => void
  /** This page's week, once it was saved and read. */
  week: WikiEntry | undefined
  bots: Bot[]
  dirty: boolean
  saved: boolean
  onSave: () => void
  onDelete: () => void
  /** Back to the pages on a phone; asks first when there are unsaved changes. */
  onBack: () => void
  backConfirming: boolean
  onBlurConfirm: () => void
}

/** One page as a writing surface: its name, whether it goes with every question, its text, and who read it. */
export function WikiEditor({ draft, onChange, week, bots, dirty, saved, onSave, onDelete, onBack, backConfirming, onBlurConfirm }: WikiEditorProps) {
  const [deleting, setDeleting] = useState(false)
  const readers = Object.entries(week?.by ?? {})
    .flatMap(([id, reads]) => {
      const bot = bots.find((b) => b.id === id)
      return bot ? [{ bot, reads }] : []
    })
    .sort((a, b) => b.reads - a.reads)
  const named = readers.slice(0, READERS_SHOWN)
  const others = (week?.reads ?? 0) - named.reduce((sum, r) => sum + r.reads, 0)
  const remove = () => {
    if (!deleting) return setDeleting(true)
    setDeleting(false)
    onDelete()
  }
  return (
    <form
      className="wiki-folha"
      onSubmit={(event) => {
        event.preventDefault()
        onSave()
      }}
    >
      <header className="wiki-folha-topo">
        <button type="button" className={`acao-texto wiki-voltar ${backConfirming ? 'confirmar' : ''}`} onBlur={onBlurConfirm} onClick={onBack}>
          {backConfirming ? t.wikiUnsavedSure : t.wikiBack}
        </button>
        <div className="wiki-tipos" role="radiogroup" aria-label={t.wikiKindLabel}>
          {[true, false].map((always) => (
            <button key={String(always)} type="button" role="radio" aria-checked={draft.always === always} className={draft.always === always ? 'ativo' : ''} onClick={() => onChange({ ...draft, always })}>
              {always ? t.wikiAlways : t.wikiMatching}
            </button>
          ))}
        </div>
        <span className="wiki-estado">{dirty ? t.unsavedChanges : saved ? t.wikiSaved : ''}</span>
        {draft.slug && (
          <button type="button" className={`acao-texto wiki-apagar ${deleting ? 'confirmar' : ''}`} onBlur={() => setDeleting(false)} onClick={remove}>
            {deleting ? t.wikiDeleteSure : t.wikiDelete}
          </button>
        )}
        <button type="submit" className="wiki-salvar" disabled={!draft.title.trim() || !draft.body.trim() || !dirty}>
          {t.wikiSave}
        </button>
      </header>
      <p className="wiki-dica">{draft.always ? t.wikiAlwaysHint : t.wikiMatchingHint}</p>
      <input className="wiki-titulo" value={draft.title} maxLength={TITLE_MAX} placeholder={t.wikiTitleLabel} aria-label={t.wikiTitleLabel} onChange={(event) => onChange({ ...draft, title: event.target.value })} />
      <textarea className="wiki-corpo" value={draft.body} maxLength={BODY_MAX} placeholder={t.wikiBodyPlaceholder} aria-label={t.wikiBodyLabel} onChange={(event) => onChange({ ...draft, body: event.target.value })} />
      <footer className="wiki-folha-pe">
        {week && week.reads > 0 ? (
          <span className="wiki-leitores">
            <span>{(week.reads === 1 ? t.wikiReadsOne : t.wikiReads).replace('{n}', String(week.reads)).replace('{t}', compact(week.tokens))}</span>
            {named.map(({ bot, reads }) => (
              <span key={bot.id} className="wiki-leitor">
                <Avatar agent={faceForBot(bot)} size={16} state="still" />
                {`${bot.name} ${reads}×`}
              </span>
            ))}
            {others > 0 && <span>{t.wikiOtherReaders.replace('{n}', String(others))}</span>}
          </span>
        ) : (
          <span className="wiki-leitores">{draft.slug ? t.wikiNoReads : t.wikiNotSaved}</span>
        )}
        <span className="wiki-conta">{`${draft.body.length}/${BODY_MAX}`}</span>
      </footer>
    </form>
  )
}
