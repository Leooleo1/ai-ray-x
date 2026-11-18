import type { Bots } from '../bots'
import { WiresEditor } from '../components/WiresEditor'
import { Frame } from './parts'
import { WiresMap } from './WiresMap'
import { o } from './texts'

interface StepWiresProps {
  bots: Bots
  onNext: () => void
}

/** Which of the new projects know of each other; nothing is wired until the person says so. */
export function StepWires({ bots, onNext }: StepWiresProps) {
  return (
    <Frame
      actions={
        <>
          <span className="bv-mono">{o.wiresLater}</span>
          <button className="bv-btn cheio bv-principal" onClick={onNext}>
            {o.continue}
          </button>
        </>
      }
    >
      <WiresMap bots={bots.state.bots} links={bots.state.links} />
      <WiresEditor bots={bots} />
    </Frame>
  )
}
