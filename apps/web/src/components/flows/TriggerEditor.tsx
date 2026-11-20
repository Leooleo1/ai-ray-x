import type { Bot, FlowTrigger } from '../../api'
import { AGENT_LABEL } from '../../avatars'
import { t } from '../../i18n'
import { dayName } from '../RoutinesEditor'

const WEEKDAYS = [0, 1, 2, 3, 4, 5, 6]
const WORKDAYS = [0, 1, 2, 3, 4]
const two = (n: number) => String(n).padStart(2, '0')

const KINDS: { kind: FlowTrigger['kind']; label: () => string }[] = [
  { kind: 'manual', label: () => t.flowManual },
  { kind: 'schedule', label: () => t.flowSchedule },
  { kind: 'task_done', label: () => t.flowTaskDone },
]

export const freshTrigger = (kind: FlowTrigger['kind']): FlowTrigger =>
  kind === 'schedule' ? { kind, days: WORKDAYS, hour: 9, minute: 0 } : kind === 'task_done' ? { kind, bot: null } : { kind }

interface TriggerEditorProps {
  trigger: FlowTrigger
  bots: Bot[]
  onChange: (trigger: FlowTrigger) => void
}

/** Which trigger, then its details: the days and time, or whose tasks. */
export function TriggerEditor({ trigger, bots, onChange }: TriggerEditorProps) {
  return (
    <>
      <div className="fluxo-opcoes" role="radiogroup" aria-label={t.flowTrigger}>
        {KINDS.map(({ kind, label }) => (
          <button key={kind} type="button" role="radio" aria-checked={trigger.kind === kind} className={trigger.kind === kind ? 'ativo' : ''} onClick={() => trigger.kind !== kind && onChange(freshTrigger(kind))}>
            {label()}
          </button>
        ))}
      </div>
      {trigger.kind === 'schedule' && (
        <div className="fluxo-quando">
          <div className="fluxo-dias" role="group" aria-label={t.routineDays}>
            {WEEKDAYS.map((d) => {
              const on = trigger.days.includes(d)
              return (
                <button key={d} type="button" aria-pressed={on} className={on ? 'ativo' : ''} onClick={() => onChange({ ...trigger, days: on ? trigger.days.filter((x) => x !== d) : [...trigger.days, d].sort() })}>
                  {dayName(d, 'short').slice(0, 3)}
                </button>
              )
            })}
          </div>
          <input
            type="time"
            className="fluxo-campo"
            aria-label={t.routineTime}
            value={`${two(trigger.hour)}:${two(trigger.minute)}`}
            onChange={(event) => {
              const [hour, minute] = event.target.value.split(':').map(Number)
              if (Number.isFinite(hour) && Number.isFinite(minute)) onChange({ ...trigger, hour, minute })
            }}
          />
        </div>
      )}
      {trigger.kind === 'task_done' && (
        <select className="fluxo-campo" aria-label={t.flowWhoseTasks} value={trigger.bot ?? ''} onChange={(event) => onChange({ ...trigger, bot: event.target.value || null })}>
          <option value="">{t.flowAnyAi}</option>
          {bots.map((bot) => (
            <option key={bot.id} value={bot.id}>{`${bot.name} · ${AGENT_LABEL[bot.agent]}`}</option>
          ))}
        </select>
      )}
      <small className="fluxo-dica">{trigger.kind === 'manual' ? t.flowManualHint : trigger.kind === 'schedule' ? t.flowScheduleHint : t.flowTaskHint}</small>
    </>
  )
}
