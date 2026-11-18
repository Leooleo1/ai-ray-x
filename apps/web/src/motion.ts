// Small motion helpers. Every effect runs once, on mount, and is skipped when
// the person asked the system for reduced motion.

import { useEffect, useRef, useState } from 'react'

const COUNT_MS = 1100

export function prefersReducedMotion(): boolean {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

/** Counts from zero to the first value it gets; later values show right away. */
export function useCountUp(target: number): number {
  const [shown, setShown] = useState(() => (prefersReducedMotion() ? target : 0))
  const started = useRef(false)
  useEffect(() => {
    if (started.current || prefersReducedMotion()) {
      setShown(target)
      return undefined
    }
    started.current = true
    const begin = performance.now()
    let frame = 0
    const tick = (now: number) => {
      const progress = Math.min(1, (now - begin) / COUNT_MS)
      const eased = 1 - (1 - progress) ** 3
      setShown(target * eased)
      if (progress < 1) frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  }, [target])
  return shown
}
