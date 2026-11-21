import { projectBots } from '../../api'
import type { Bots } from '../../bots'
import { t } from '../../i18n'
import { WiresMap } from '../../onboarding/WiresMap'
import { WiresEditor } from '../WiresEditor'

/** The projects as a map, lit where wired, beside the switches that wire them. */
export function WiresPanel({ bots }: { bots: Bots }) {
  const real = projectBots(bots.state.bots)
  const projects = new Set(real.map((b) => b.name)).size
  if (projects < 2) return <p className="cfg-vazio">{t.noWires}</p>
  return (
    <div className="cfg-fios">
      <WiresMap bots={real} links={bots.state.links} />
      <section className="cartao cfg-fios-lista">
        <small>{t.wiresHint}</small>
        <WiresEditor bots={bots} />
      </section>
    </div>
  )
}
