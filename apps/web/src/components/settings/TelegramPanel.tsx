import { type FormEvent, useEffect, useState } from 'react'
import { type TelegramState, fetchTelegram, telegramAction } from '../../api'
import { t } from '../../i18n'

/** How often the card looks for the pairing (it happens in Telegram). */
const POLL_MS = 3000

const messageOf = (error: unknown) => (error instanceof Error ? error.message : String(error))

/** The Telegram bridge: connect a bot from @BotFather, pair one chat, see who answers. */
export function TelegramPanel() {
  const [state, setState] = useState<TelegramState | null>(null)
  const [token, setToken] = useState('')
  const [busy, setBusy] = useState(false)
  const [forgetting, setForgetting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const waitingPair = Boolean(state?.connected && !state.paired)
  useEffect(() => {
    let alive = true
    const load = () => fetchTelegram().then((next) => alive && setState(next)).catch((failure: unknown) => alive && setError(messageOf(failure)))
    void load()
    // While a code waits, the pairing shows up here without a reload.
    const timer = waitingPair ? window.setInterval(() => void load(), POLL_MS) : undefined
    return () => {
      alive = false
      window.clearInterval(timer)
    }
  }, [waitingPair])
  const act = async (action: 'connect' | 'pair' | 'forget', body: Record<string, unknown> = {}) => {
    setBusy(true)
    setError(null)
    try {
      setState(await telegramAction(action, body))
      setToken('')
    } catch (failure: unknown) {
      setError(messageOf(failure))
    } finally {
      setBusy(false)
      setForgetting(false)
    }
  }
  const connect = (event: FormEvent) => {
    event.preventDefault()
    if (token.trim()) void act('connect', { token })
  }
  if (!state) return null
  const bot = state.username ?? ''
  return (
    <article className="cfg-ia telegram">
      <header>
        <span className="cfg-nome">
          <b>{t.telegramTitle}</b>
          <small>{state.paired ? t.telegramPaired.replace('{name}', state.chat_name ?? '').replace('{bot}', bot) : t.telegramHint}</small>
        </span>
        {state.connected && (
          <button type="button" className={`vidro-btn ${forgetting ? 'confirmar' : ''}`} disabled={busy} onBlur={() => setForgetting(false)} onClick={() => (forgetting ? void act('forget') : setForgetting(true))}>
            {forgetting ? t.telegramForgetSure : t.telegramForget}
          </button>
        )}
      </header>
      {!state.connected && (
        <form className="telegram-token" onSubmit={connect}>
          <p className="cfg-faz">{t.telegramSteps}</p>
          <label className="ajustes-campo">
            <span>{t.telegramToken}</span>
            <input type="password" value={token} autoComplete="off" spellCheck={false} placeholder="123456789:AA…" onChange={(event) => setToken(event.target.value)} />
          </label>
          <span className="acao">
            <button type="submit" className="cheio" disabled={busy || !token.trim()}>
              {t.telegramConnect}
            </button>
          </span>
        </form>
      )}
      {state.connected && !state.paired && (
        <div className="telegram-par">
          <p className="cfg-faz">{t.telegramPairHint}</p>
          {state.code ? (
            <>
              <span className="celular-codigo">{state.code}</span>
              <span className="acao">
                <a className="cheio" href={`https://t.me/${bot}?start=${state.code}`} target="_blank" rel="noreferrer">
                  {t.telegramOpen.replace('{bot}', bot)}
                </a>
              </span>
            </>
          ) : (
            <span className="acao">
              <button type="button" className="cheio" disabled={busy} onClick={() => void act('pair')}>
                {t.phonePairNew}
              </button>
            </span>
          )}
        </div>
      )}
      {state.paired && (
        <>
          <p className="cfg-faz">{t.telegramAnswers}</p>
          <span className="acao">
            <button type="button" className="vidro-btn" disabled={busy} onClick={() => void act('pair')}>
              {t.telegramPairAgain}
            </button>
          </span>
        </>
      )}
      {error && <p className="nota failed quebra">{error}</p>}
    </article>
  )
}
