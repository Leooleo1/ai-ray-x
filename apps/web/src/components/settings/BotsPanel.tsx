import type { Bot } from '../../api'
import { AGENT_LABEL, faceForBot, stageLight } from '../../avatars'
import type { Bots } from '../../bots'
import { t } from '../../i18n'
import { Avatar } from '../Avatar'
import { modelName } from '../BotSettings'
import { AGENTS } from '../../agents'
import { useTipPrefs } from '../../tips'
import { HOUSE } from '../chat/HouseLine'
import '../controls.css'

const count = (n: number, [one, many]: [string, string]) => `${n} ${n === 1 ? one : many}`

interface BotsPanelProps {
  bots: Bots
  onOpenBot: (id: string) => void
  onAddBot: () => void
}

/** Each bot as a card on its own light: what it runs on, how much it holds, and the way into its chat. */
export function BotsPanel({ bots, onOpenBot, onAddBot }: BotsPanelProps) {
  const wiresOf = (bot: Bot) => bots.state.links.filter(([a, b]) => a === bot.name || b === bot.name).length
  return (
    <>
    <TipsCard />
    <div className="cfg-grade">
      {bots.state.bots.map((bot) => {
        const face = faceForBot(bot)
        return (
          <article key={bot.id} className="cfg-bot">
            <div className="cfg-bot-palco" style={{ backgroundImage: `url(${stageLight(face.color)})` }}>
              <Avatar agent={face} size={56} state={bot.busy ? 'thinking' : 'still'} />
            </div>
            <div className="cfg-bot-texto">
              <b>{bot.name}</b>
              <small>{[bot.persona?.name, AGENT_LABEL[bot.agent], bot.model ? modelName(bot.model) : '', bot.effort ?? ''].filter(Boolean).join(' · ')}</small>
              <small>{[count(bot.threads.length + 1, t.conversationsCount), count(bot.routines.length, t.routinesCount), count(wiresOf(bot), t.wiresCount)].join(' · ')}</small>
            </div>
            <footer>
              <button type="button" className="cfg-btn cheio" onClick={() => onOpenBot(bot.id)}>
                {t.talk}
              </button>
              <button type="button" className="lado-link" disabled={bot.busy} onClick={() => void bots.act('deactivate', { id: bot.id })}>
                {t.deactivate}
              </button>
            </footer>
          </article>
        )
      })}
      <button type="button" className="cfg-bot-novo" onClick={onAddBot}>
        <i aria-hidden="true">
          <svg viewBox="0 0 24 24">
            <path d="M12 5v14M5 12h14" />
          </svg>
        </i>
        <b>{t.addBot}</b>
        <small>{t.addBotHint}</small>
      </button>
    </div>
    </>
  )
}

/** Which house agents may float tips: one switch each. */
function TipsCard() {
  const [prefs, setPrefs] = useTipPrefs()
  return (
    <section className="cartao cfg-dicas">
      <span className="cfg-dicas-titulo">
        <b>{t.tipsTitle}</b>
        <small>{t.tipsHint}</small>
      </span>
      <div className="cfg-dicas-chaves">
        {HOUSE.map((id) => (
          <button key={id} type="button" role="switch" aria-checked={prefs[id]} aria-label={AGENTS[id].name} className={`config-chave ${prefs[id] ? 'ligada' : ''}`} onClick={() => setPrefs({ ...prefs, [id]: !prefs[id] })}>
            <i />
            {AGENTS[id].name}
          </button>
        ))}
      </div>
    </section>
  )
}
