import { t } from '../i18n'

/** What a device sees before it is paired, or after it was removed. */
export function PairScreen({ message }: { message: string | null }) {
  return (
    <main className="parear">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <rect x="6.5" y="2.5" width="11" height="19" rx="2.5" />
        <path d="M10.5 18.5h3" />
      </svg>
      <b>{t.pairTitle}</b>
      <p>{t.pairHint}</p>
      {message && <p className="nota failed">{t.pairFailed.replace('{e}', message)}</p>}
    </main>
  )
}
