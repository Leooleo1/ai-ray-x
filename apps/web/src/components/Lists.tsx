import type { Row } from '../model'
import { compact, percent, t } from '../i18n'

const DISPLAY_NAMES: Record<string, string> = {
  'your prompts': t.yourPrompts,
  'injected text': t.injected,
  '~': t.home,
}
const SECONDARY_SECRETS = ['searched', 'loaded', 'written', 'mentioned'] as const

function displayName(name: string): string {
  if (DISPLAY_NAMES[name]) return DISPLAY_NAMES[name]
  if (name.startsWith('mcp:')) return `MCP · ${name.slice(4)}`
  return name
}

interface RowsCardProps {
  title: string
  hint?: string
  rows: Row[]
  className: string
}

/** Plain rows: source dot, name, value and share, without a card around each item. */
export function RowsCard({ title, hint, rows, className }: RowsCardProps) {
  return (
    <section className={`cartao ${className}`}>
      <h2>
        {title}
        {hint && <small>{hint}</small>}
      </h2>
      <ul className="lista">
        {rows.map((row) => (
          <li key={`${row.source}-${row.name}`}>
            <i className={`ponto ${row.source}`} />
            <span className="nome">{displayName(row.name)}</span>
            <span className="num">{compact(row.value)}</span>
            <span className="apoio">{row.share !== undefined ? percent(row.share) : ''}</span>
          </li>
        ))}
      </ul>
    </section>
  )
}

interface SecretsCardProps {
  totals: Record<string, number>
}

/** Counts only: how tool calls touched secrets files. The text is never kept. */
export function SecretsCard({ totals }: SecretsCardProps) {
  const printed = totals.printed ?? 0
  return (
    <section className="cartao segredos">
      <h2>{t.secrets}</h2>
      <div className="destaque">
        <strong>{compact(printed)}</strong>
        <span>
          {printed > 0 && <i className="ponto alerta" />}
          {t.printed}
        </span>
      </div>
      <ul className="lista simples">
        {SECONDARY_SECRETS.filter((kind) => (totals[kind] ?? 0) > 0).map((kind) => (
          <li key={kind}>
            <span className="nome">{t[kind]}</span>
            <span className="num">{compact(totals[kind])}</span>
          </li>
        ))}
      </ul>
    </section>
  )
}
