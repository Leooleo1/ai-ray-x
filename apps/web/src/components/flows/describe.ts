import type { Bot, Channel, FlowStep, FlowTrigger } from '../../api'
import { t } from '../../i18n'
import { daysLabel } from '../RoutinesEditor'

const two = (n: number) => String(n).padStart(2, '0')

/** A trigger in one line: "weekdays at 09:00", "when a task of X finishes". */
export function triggerLine(trigger: FlowTrigger, bots: Bot[]): string {
  if (trigger.kind === 'manual') return t.flowManualLine
  if (trigger.kind === 'schedule') return t.flowScheduleLine.replace('{d}', daysLabel(trigger.days)).replace('{h}', `${two(trigger.hour)}:${two(trigger.minute)}`)
  const bot = bots.find((b) => b.id === trigger.bot)
  return bot ? t.flowTaskOfLine.replace('{b}', bot.name) : t.flowTaskAnyLine
}

/** A step's title: who answers, that it waits for the person, or where it posts. */
export function stepTitle(step: FlowStep, bots: Bot[], channels: Channel[]): string {
  if (step.kind === 'ask') {
    const bot = bots.find((b) => b.id === step.bot)
    return t.flowAskTitle.replace('{b}', bot?.name ?? '…')
  }
  if (step.kind === 'approve') return t.flowApproveTitle
  const channel = channels.find((c) => c.id === step.channel)
  return t.flowPostTitle.replace('{c}', channel?.name ?? '…')
}

/** What the step still needs before the workflow can be saved; null when it is complete. */
export function stepMissing(step: FlowStep): string | null {
  if (step.kind === 'ask') return !step.bot ? t.flowNeedsAi : !step.text.trim() ? t.flowNeedsQuestion : null
  if (step.kind === 'post') return !step.channel ? t.flowNeedsChannel : !step.text.trim() ? t.flowNeedsPost : null
  return null
}

export function triggerMissing(trigger: FlowTrigger): string | null {
  return trigger.kind === 'schedule' && trigger.days.length === 0 ? t.flowNeedsDay : null
}
