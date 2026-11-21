import { Frame } from './parts'
import { o } from './texts'
import { LanguagePicker } from '../components/LanguagePicker'
import { IS_REMOTE } from '../api'
import { t } from '../i18n'

interface StepWelcomeProps {
  checking: boolean
  picking: boolean
  opening: boolean
  onChoose: () => void
  onAdvanced: () => void
}

export function StepWelcome({ checking, picking, opening, onChoose, onAdvanced }: StepWelcomeProps) {
  return (
    <Frame
      actions={
        <>
          <span className="bv-selo">
            <i className="ponto ok" />
            {o.localOnly}
          </span>
          <LanguagePicker />
          <button type="button" className={`bv-btn ${IS_REMOTE ? 'cheio bv-principal' : 'link'}`} disabled={picking || opening} onClick={onAdvanced}>
            {o.advancedSetup}
          </button>
          {!IS_REMOTE && (
            <button type="button" className="bv-btn cheio bv-principal" disabled={checking || picking || opening} onClick={onChoose}>
              {opening ? o.quickOpening : picking ? t.choosingProjectFolder : checking ? o.checking : t.chooseProjectFolder}
            </button>
          )}
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
