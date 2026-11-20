import { useState } from 'react'
import type { Bot } from '../../api'
import { AGENT_LABEL, faceForBot, stageLight, personaName } from '../../avatars'
import type { Bots } from '../../bots'
import { t } from '../../i18n'
import { Avatar } from '../Avatar'
import { modelName } from '../BotSettings'
import '../controls.css'

const count = (n: number, [one, many]: [string, string]) => `${n} ${n === 1 ? one : many}`

interface BotsPanelProps {
  bots: Bots
  onOpenBot: (id: string) => void
  onAddBot: () => void
}

/** Each bot as a card on its own light: what it runs on, how much it holds, and the way into its chat. */
export function BotsPanel({ bots, onOpenBot, onAddBot }: BotsPanelProps) {
  const [deactivating, setDeactivating] = useState<string | null>(null)
  const wiresOf = (bot: Bot) => bots.state.links.filter(([a, b]) => a === bot.name || b === bot.name).length
  return (
    <>
    <div className="cfg-grade">
      {bots.state.bots.map((bot) => {
        const face = faceForBot(bot)
        const confirming = deactivating === bot.id
        return (
          <article key={bot.id} className="cfg-bot">
            <div className="cfg-bot-palco" style={{ backgroundImage: `url(${stageLight(face.color)})` }}>
              <Avatar agent={face} size={56} state={bot.busy ? 'thinking' : 'still'} />
            </div>
            <div className="cfg-bot-texto">
              <b>{bot.name}</b>
              <small>{[personaName(bot), AGENT_LABEL[bot.agent], bot.model ? modelName(bot.model) : '', bot.effort ?? ''].filter(Boolean).join(' · ')}</small>
              <small>{[count(bot.threads.length + 1, t.conversationsCount), count(bot.routines.length, t.routinesCount), count(wiresOf(bot), t.wiresCount)].join(' · ')}</small>
            </div>
            <footer>
              <button type="button" className="cfg-btn cheio" onClick={() => onOpenBot(bot.id)}>
                {t.talk}
              </button>
              <button
                type="button"
                className={`lado-link ${confirming ? 'confirmar' : ''}`}
                disabled={bot.busy}
                onBlur={() => setDeactivating(null)}
                onClick={() => {
                  if (!confirming) setDeactivating(bot.id)
                  else {
                    setDeactivating(null)
                    void bots.act('deactivate', { id: bot.id })
                  }
                }}
              >
                {confirming ? t.deactivateSure : t.deactivate}
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

