import { Frame } from './parts'
import { o } from './texts'

export function StepWelcome({ onNext }: { onNext: () => void }) {
  return (
    <Frame
      actions={
        <>
          <span className="bv-selo">
            <i className="ponto ok" />
            {o.localOnly}
          </span>
          <button className="bv-btn cheio bv-principal" onClick={onNext}>
            {o.start}
          </button>
        </>
      }
    >
      <ol className="bv-passos">
        {o.welcomeSteps.map(([title, detail], i) => (
          <li key={title}>
            <span className="bv-num">{String(i + 1).padStart(2, '0')}</span>
            <b>{title}</b>
            <span>{detail}</span>
          </li>
        ))}
      </ol>
    </Frame>
  )
}
