import type { Batch, Bot, BotMessage } from '../../api'
import { AGENT_LABEL, faceForBot } from '../../avatars'
import type { Bots } from '../../bots'
import { compact, t } from '../../i18n'
import { Avatar } from '../Avatar'
import { Markdown } from '../Markdown'
import { clock } from './BotLine'
import type { ChatNav } from './nav'

interface BatchViewProps {
  batch: Batch
  bots: Bots
  nav: ChatNav
}

/** The last answer or failure in a conversation. */
function outcome(messages: BotMessage[]): BotMessage | undefined {
  return [...messages].reverse().find((m) => m.author === 'bot' || m.kind === 'failed' || m.kind === 'stopped' || m.kind === 'interrupted')
}

/** One question asked to several bots: one column per bot, each with its answer and what it read. */
export function BatchView({ batch, bots, nav }: BatchViewProps) {
  const columns = batch.items.map((item) => ({ item, bot: bots.state.bots.find((b) => b.id === item.bot) }))
  const skipped = batch.skipped.map((id) => bots.state.bots.find((b) => b.id === id)?.name ?? id)
  return (
    <main className="chat">
      <section className="cartao chat-centro bot-chat sem-lista">
        <div className="conversa">
          <header className="chat-topo bot-topo grupo-topo">
            <button type="button" className="icone-btn" aria-label={t.askMany} title={t.askMany} onClick={nav.askMany}>
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M15 6l-6 6 6 6" />
              </svg>
            </button>
            <b className="grupo-titulo">{batch.text}</b>
            <small className="grupo-meta">{[clock(batch.created_ms), skipped.length > 0 ? `${skipped.join(', ')}: ${t.leftOut}` : ''].filter(Boolean).join(' · ')}</small>
            <button type="button" className="icone-btn" title={t.removeGroup} aria-label={t.removeGroup} onClick={() => void bots.act('batch_delete', { batch: batch.id }).then(nav.askMany)}>
              ×
            </button>
          </header>
          <div className="grupo-colunas">
            {columns.map(({ item, bot }) => (bot ? <Column key={item.bot} bot={bot} thread={item.thread} nav={nav} /> : null))}
          </div>
        </div>
      </section>
    </main>
  )
}

interface ColumnProps {
  bot: Bot
  thread: string
  nav: ChatNav
}

function Column({ bot, thread, nav }: ColumnProps) {
  const conversation = bot.threads.find((x) => x.id === thread)
  const working = bot.busy && bot.busy_thread === thread
  const last = conversation ? outcome(conversation.messages) : undefined
  const reply = last?.data.reply
  const face = faceForBot(bot)
  return (
    <article className="grupo-coluna">
      <header>
        <Avatar agent={face} size={24} state={working ? 'thinking' : 'still'} />
        <b>{bot.name}</b>
        <small>{AGENT_LABEL[bot.agent]}</small>
      </header>
      <div className="grupo-resposta">
        {!conversation && <p className="nota">{t.threadClosed}</p>}
        {working && (
          <p className="grupo-passo">
            {bot.step ? t.stepWords[bot.step.kind] : t.answering}
            {bot.step?.target && <code className="passo-alvo">{bot.step.target}</code>}
          </p>
        )}
        {!working && reply && <Markdown text={[reply.text, reply.question].filter(Boolean).join('\n\n')} />}
        {!working && last?.kind === 'failed' && <p className="nota failed">{last.data.error}</p>}
        {!working && last?.kind === 'stopped' && <p className="nota">{t.stoppedNote}</p>}
        {!working && last?.kind === 'interrupted' && <p className="nota">{t.interruptedNote}</p>}
      </div>
      <footer>
        <small>{reply ? `${t.readWord} ${compact(reply.read_tokens)} tokens` : ''}</small>
        {conversation && (
          <button type="button" className="lado-link" onClick={() => nav.openMessage(bot.id, thread, last?.ms ?? 0)}>
            {t.openConversation}
          </button>
        )}
      </footer>
    </article>
  )
}
