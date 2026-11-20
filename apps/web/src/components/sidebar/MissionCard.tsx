import { useState } from 'react'
import type { FirstUseMilestone, FirstUseMission } from '../../api'
import { t } from '../../i18n'
import { Icon } from '../Icons'

const STEPS: FirstUseMission[] = ['chat', 'approval', 'task', 'link', 'usage']

interface MissionCardProps {
  completed: FirstUseMilestone[]
  actions: Record<FirstUseMission, () => void>
  onDismiss: () => Promise<unknown>
}

/** First-use shortcuts become a checklist as the person completes each step. */
export function MissionCard({ completed, actions, onDismiss }: MissionCardProps) {
  const [saving, setSaving] = useState(false)
  if (STEPS.every((step) => completed.includes(step))) return null
  const dismiss = async () => {
    setSaving(true)
    await onDismiss()
    setSaving(false)
  }
  return (
    <section className="lado-missoes" aria-label={t.missionsTitle}>
      <header>
        <b>{t.missionsTitle}</b>
        <button type="button" disabled={saving} onClick={() => void dismiss()} aria-label={t.missionsDismiss} title={t.missionsDismiss}>
          <Icon name="close" />
        </button>
      </header>
      <ul>
        {STEPS.map((step) => (
          <li key={step}>
            {completed.includes(step) ? (
              <span className="lado-missao-feita">
                <Icon name="check" />
                {t.missions[step]}
                <span className="lado-missao-status">{t.missionComplete}</span>
              </span>
            ) : (
              <button type="button" onClick={actions[step]} disabled={step === 'usage' && !completed.includes('turn')}>
                <i aria-hidden="true" />
                {t.missions[step]}
              </button>
            )}
          </li>
        ))}
      </ul>
    </section>
  )
}
