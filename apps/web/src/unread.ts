// Which bots answered while the person looked elsewhere, and a browser
// notice when one finishes out of sight. What was seen is kept per bot.

import { useEffect, useMemo, useRef, useState } from 'react'
import type { Bot } from './api'

const SEEN_KEY = 'raiox.seen'

/** What was seen; the first time ever, everything already there counts as seen. */
const lastAnswer = (bot: Bot) => [...bot.messages].reverse().find((m) => m.author === 'bot')?.ms ?? 0

function loadSeen(bots: Bot[]): Record<string, number> {
  try {
    const kept = localStorage.getItem(SEEN_KEY)
    if (kept) return JSON.parse(kept) as Record<string, number>
  } catch {
    // A broken entry starts over like a first visit.
  }
  const first = Object.fromEntries(bots.map((b) => [b.id, lastAnswer(b)]))
  localStorage.setItem(SEEN_KEY, JSON.stringify(first))
  return first
}


/** Asks once for permission to notify, from a click (browsers require one). */
export function askToNotify() {
  if ('Notification' in window && Notification.permission === 'default') void Notification.requestPermission()
}

/** The bots with an answer the person has not seen; the open one counts as seen. */
export function useUnread(bots: Bot[], openId: string | null): Set<string> {
  const [seen, setSeen] = useState(() => loadSeen(bots))
  const wasBusy = useRef(new Map<string, boolean>())

  // The open chat is seen as soon as it shows (and again when a new answer lands in it).
  const openLast = bots.find((b) => b.id === openId)
  const openMs = openLast ? lastAnswer(openLast) : 0
  useEffect(() => {
    if (!openId || !openMs) return
    setSeen((current) => {
      if ((current[openId] ?? 0) >= openMs) return current
      const next = { ...current, [openId]: openMs }
      localStorage.setItem(SEEN_KEY, JSON.stringify(next))
      return next
    })
  }, [openId, openMs])

  // A bot that just finished out of sight gets a browser notice.
  useEffect(() => {
    for (const bot of bots) {
      const before = wasBusy.current.get(bot.id)
      wasBusy.current.set(bot.id, bot.busy)
      const outOfSight = bot.id !== openId || document.hidden
      if (before && !bot.busy && outOfSight && 'Notification' in window && Notification.permission === 'granted') {
        const answer = [...bot.messages].reverse().find((m) => m.author === 'bot')
        const text = answer?.data.reply?.text ?? answer?.data.error ?? ''
        new Notification(`${bot.persona?.name ?? bot.name} · ${bot.name}`, { body: text.slice(0, 140), tag: bot.id })
      }
    }
  }, [bots, openId])

  const unread = useMemo(
    () => new Set(bots.filter((b) => b.id !== openId && lastAnswer(b) > (seen[b.id] ?? 0) && !b.busy).map((b) => b.id)),
    [bots, openId, seen],
  )
  // The tab title counts them, so a background tab shows there is something to read.
  useEffect(() => {
    document.title = unread.size > 0 ? `(${unread.size}) raiox` : 'raiox'
  }, [unread.size])
  return unread
}
