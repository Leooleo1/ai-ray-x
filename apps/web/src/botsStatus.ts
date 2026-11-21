import type { BotsState, BotsStatus } from './api'

/** Preserve the loaded histories when this poll changes only live activity. */
export function mergeBotsStatus(state: BotsState, status: BotsStatus): BotsState | null {
  if (state.chat_revision !== status.chat_revision || state.bots.length !== status.bots.length || state.channels.length !== status.channels.length) return null
  const bots = new Map(status.bots.map((bot) => [bot.id, bot]))
  const channels = new Map(status.channels.map((channel) => [channel.id, channel.working]))
  if (state.bots.some((bot) => {
    const live = bots.get(bot.id)
    if (bot.busy_threads?.some((job) => !live?.busy_threads?.some((next) => next.thread === job.thread && next.started_ms === job.started_ms))) return true
    return !live || (bot.busy && (!live.busy || bot.started_ms !== live.started_ms || bot.busy_thread !== live.busy_thread))
  }) || state.channels.some((channel) => !channels.has(channel.id) || (channel.working && !channels.get(channel.id)))) return null
  return {
    ...state,
    bots: state.bots.map((bot) => {
      const live = bots.get(bot.id)!
      return live.busy ? { ...bot, ...live } : bot
    }),
    channels: state.channels.map((channel) => {
      const working = channels.get(channel.id)!
      return channel.working === working ? channel : { ...channel, working }
    }),
  }
}
