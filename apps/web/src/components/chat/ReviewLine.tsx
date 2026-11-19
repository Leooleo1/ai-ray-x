import type { Bot, BotMessage } from '../../api'
import { AGENT_LABEL, faceForBot } from '../../avatars'
import { compact, t } from '../../i18n'
import { Markdown } from '../Markdown'

const VERDICT_CLASS: Record<string, string> = { ok: 'ok', concerns: 'ressalva', reject: 'contra' }

/** A review of another bot's proposal, or the line saying one was asked. */
export function ReviewLine({ message, reviewer }: { message: BotMessage; reviewer: Bot | undefined }) {
  const name = reviewer ? AGENT_LABEL[reviewer.agent] : message.data.agent ? AGENT_LABEL[message.data.agent] : ''
  if (message.kind === 'review_asked') return <p className="linha-sistema">{t.reviewAsked.replace('{a}', name)}</p>
  const reply = message.data.reply
  if (!reply) return null
  const verdict = reply.verdict ?? ''
  return (
    <div className="linha de-agente revisao" id={`msg-${message.ms}`}>
      <div className="corpo">
        <div className="revisao-cartao">
          <header>
            <i className="ponto" style={reviewer ? { background: faceForBot(reviewer).color } : undefined} aria-hidden="true" />
            <b>{t.reviewBy.replace('{a}', name)}</b>
            {verdict && <span className={`veredito ${VERDICT_CLASS[verdict] ?? ''}`}>{t.verdicts[verdict] ?? verdict}</span>}
          </header>
          <Markdown text={reply.text} />
        </div>
        <span className="bot-meta">{`${t.readWord} ${compact(reply.read_tokens)} tokens · ${Math.max(1, Math.round(reply.duration_ms / 1000))} s`}</span>
      </div>
    </div>
  )
}
