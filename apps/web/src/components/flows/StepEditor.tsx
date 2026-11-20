import type { Bot, Channel, FlowStep } from '../../api'
import { AGENT_LABEL } from '../../avatars'
import { t } from '../../i18n'

const MAX_TEXT = 4000

interface StepEditorProps {
  step: FlowStep
  bots: Bot[]
  channels: Channel[]
  onChange: (step: FlowStep) => void
}

/** A step's details: which AI and what to ask, a note for the approval, or the channel and the post. */
export function StepEditor({ step, bots, channels, onChange }: StepEditorProps) {
  if (step.kind === 'approve') {
    return (
      <>
        <input className="fluxo-campo" value={step.text} maxLength={MAX_TEXT} placeholder={t.flowApproveHint} onChange={(event) => onChange({ ...step, text: event.target.value })} />
        <small className="fluxo-dica">{t.flowApproveExplain}</small>
      </>
    )
  }
  const text = (
    <textarea
      className="fluxo-campo fluxo-texto"
      rows={4}
      value={step.text}
      maxLength={MAX_TEXT}
      placeholder={step.kind === 'ask' ? t.flowAskHint : t.flowPostHint}
      onChange={(event) => onChange({ ...step, text: event.target.value })}
    />
  )
  return (
    <>
      {step.kind === 'ask' ? (
        <select className="fluxo-campo" aria-label={t.flowWhichAi} value={step.bot} onChange={(event) => onChange({ ...step, bot: event.target.value })}>
          <option value="" disabled>
            {t.flowWhichAi}
          </option>
          {bots.map((bot) => (
            <option key={bot.id} value={bot.id}>{`${bot.name} · ${AGENT_LABEL[bot.agent]}`}</option>
          ))}
        </select>
      ) : (
        <select className="fluxo-campo" aria-label={t.flowWhichChannel} value={step.channel} onChange={(event) => onChange({ ...step, channel: event.target.value })}>
          <option value="" disabled>
            {t.flowWhichChannel}
          </option>
          {channels.map((channel) => (
            <option key={channel.id} value={channel.id}>{`#${channel.name}`}</option>
          ))}
        </select>
      )}
      {text}
      <small className="fluxo-dica">{t.flowPlaceholders}</small>
    </>
  )
}
