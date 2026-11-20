import { useEffect, useState } from 'react'
import { type ConversationContext, fetchConversationContext } from '../../api'
import { compact, percent, t } from '../../i18n'

/** Read again this often while the conversation is open, and when a turn ends. */
const POLL_MS = 20_000
/** Past these shares of the compaction point the meter warns, then alarms. */
const WARN = 0.6
const ALARM = 0.85
/** How many of the biggest things loaded the detail lists. */
const TOP_LOADED = 3
/** The index names two kinds of loaded text in English; the rest are tool names. */
const nameOf = (name: string) => (name === 'your prompts' ? t.contextPrompts : name === 'injected text' ? t.contextInjected : name)

interface ContextMeterProps {
  bot: string
  thread: string
  busy: boolean
  onFresh: () => void
}

/** How much this conversation re-reads on every request, against where it compacts; the detail on hover or click. */
export function ContextMeter({ bot, thread, busy, onFresh }: ContextMeterProps) {
  const [context, setContext] = useState<ConversationContext | null>(null)
  const [hovered, setHovered] = useState(false)
  const [pinned, setPinned] = useState(false)
  useEffect(() => {
    let alive = true
    const load = () => fetchConversationContext(bot, thread).then((next) => alive && setContext(next)).catch(() => undefined)
    setContext(null)
    void load()
    const timer = window.setInterval(() => void load(), POLL_MS)
    return () => {
      alive = false
      window.clearInterval(timer)
    }
  }, [bot, thread, busy])
  useEffect(() => {
    if (!pinned) return undefined
    const away = (event: PointerEvent) => !(event.target as Element).closest('.contexto') && setPinned(false)
    window.addEventListener('pointerdown', away)
    return () => window.removeEventListener('pointerdown', away)
  }, [pinned])
  if (!context) return null
  const { status, compact_at: at } = context
  const limit = at ?? context.window
  const share = limit ? Math.min(1, status.context_tokens / limit) : 0
  const level = share >= ALARM ? 'alarme' : share >= WARN ? 'aviso' : 'ok'
  const loaded = [...status.loaded_since_compaction].sort((a, b) => b.tokens - a.tokens).slice(0, TOP_LOADED)
  const where = context.compact_from === 'setting' ? t.contextAtSetting : context.compact_from === 'usual' ? t.contextAtUsual : t.contextAtLimit
  return (
    <div className="contexto" onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}>
      <button type="button" className={`contexto-botao ${level}`} aria-expanded={hovered || pinned} title={t.contextTitle} onClick={() => setPinned(!pinned)}>
        <span className="contexto-barra" aria-hidden="true">
          <i style={{ width: `${Math.max(4, share * 100)}%` }} />
        </span>
        <span>{compact(status.context_tokens)}</span>
      </button>
      {(hovered || pinned) && (
        <div className="contexto-painel" role="dialog" aria-label={t.contextTitle}>
          <b>{t.contextReads.replace('{n}', compact(status.context_tokens))}</b>
          {limit && <small>{(at ? where : t.contextAtLimit).replace('{n}', compact(limit)).replace('{p}', percent(share))}</small>}
          <dl>
            <dt>{t.contextRequests}</dt>
            <dd>{status.requests_since_compaction}</dd>
            <dt>{t.contextStartup}</dt>
            <dd>{compact(status.startup_tokens)}</dd>
            {loaded.map((item) => (
              <div key={item.name} className="contexto-item">
                <dt>{nameOf(item.name)}</dt>
                <dd>{compact(item.tokens)}</dd>
              </div>
            ))}
          </dl>
          <p>{t.contextHint}</p>
          <button
            type="button"
            className="contexto-novo"
            onClick={() => {
              setPinned(false)
              setHovered(false)
              onFresh()
            }}
          >
            {t.contextFresh}
          </button>
        </div>
      )}
    </div>
  )
}
