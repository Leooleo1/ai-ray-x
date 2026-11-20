/** Line icons for the workflow nodes, in the panel's one stroke. */
const PATHS = {
  trigger: ['M13 3L5 13.5h6L10 21l8-10.5h-6z'],
  ask: ['M5 6.5A2.5 2.5 0 0 1 7.5 4h9A2.5 2.5 0 0 1 19 6.5v6a2.5 2.5 0 0 1-2.5 2.5H11l-4 3.5V15A2.5 2.5 0 0 1 5 12.5z'],
  approve: ['M12 3.5l7 3v5c0 4.2-3 7.6-7 9-4-1.4-7-4.8-7-9v-5z', 'M9 12l2.2 2.2L15.5 10'],
  post: ['M9 4L7 20', 'M17 4l-2 16', 'M4.5 9h15', 'M3.5 15h15'],
  plus: ['M12 5v14', 'M5 12h14'],
} as const

export function FlowIcon({ name }: { name: keyof typeof PATHS }) {
  return (
    <svg className="fluxo-svg" viewBox="0 0 24 24" aria-hidden="true">
      {PATHS[name].map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  )
}
