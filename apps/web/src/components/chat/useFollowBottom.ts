import { type RefObject, useCallback, useEffect, useRef, useState } from 'react'

/** Within this many pixels of the end, the reader counts as at the bottom. */
const NEAR_END = 56

/**
 * Keeps a list at its newest line while the reader is there; reading further up,
 * new lines do not pull the view, and `unseen` says something arrived below.
 */
export function useFollowBottom(list: RefObject<HTMLElement | null>, size: number, chat: string) {
  const [unseen, setUnseen] = useState(false)
  const atEnd = useRef(true)

  const jump = useCallback(
    (smooth = true) => {
      const el = list.current
      if (!el) return
      el.scrollTo({ top: el.scrollHeight, behavior: smooth ? 'smooth' : 'auto' })
      atEnd.current = true
      setUnseen(false)
    },
    [list],
  )

  useEffect(() => {
    const el = list.current
    if (!el) return undefined
    const onScroll = () => {
      atEnd.current = el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_END
      if (atEnd.current) setUnseen(false)
    }
    // An image that loads after the jump (a screenshot, an attached picture) pushes the end down: follow it.
    const onLoad = () => {
      if (atEnd.current) el.scrollTo({ top: el.scrollHeight })
    }
    el.addEventListener('scroll', onScroll, { passive: true })
    el.addEventListener('load', onLoad, true)
    return () => {
      el.removeEventListener('scroll', onScroll)
      el.removeEventListener('load', onLoad, true)
    }
  }, [list])

  // A new chat opens at its end, at once.
  useEffect(() => jump(false), [chat, jump])
  useEffect(() => {
    if (atEnd.current) jump(true)
    else setUnseen(true)
  }, [size, jump])

  return { unseen, jump }
}
