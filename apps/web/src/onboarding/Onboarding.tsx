import { useCallback, useEffect, useState } from 'react'
import { type Bot, type BotAgent, type GitHubState, type SetupState, botAction, fetchGitHub, fetchSetup, finishSetup } from '../api'
import { AGENT_LABEL, faceOf, stageLight } from '../avatars'
import { Avatar, type AvatarState } from '../components/Avatar'
import { Mark } from '../components/Sidebar'
import './onboarding.css'
import { PRESETS, type BotDesign, designFrom, personaOf } from './presets'
import { StepAis } from './StepAis'
import { StepBot } from './StepBot'
import { StepDone } from './StepDone'
import { StepGitHub } from './StepGitHub'
import { StepProjects } from './StepProjects'
import { StepWelcome } from './StepWelcome'
import { o } from './texts'

type Step = 'welcome' | 'ais' | 'github' | 'projects' | 'bot' | 'done'
const ORDER: Step[] = ['welcome', 'ais', 'github', 'projects', 'bot', 'done']
/** While a clone runs, the setup is asked again this often. */
const CLONE_POLL_MS = 1500
const STAGE_SIZE = 168
const STAGE_SIZE_PHONE = 112
const PHONE = '(max-width: 760px)'

const errorText = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure))

interface OnboardingProps {
  onFinish: (firstBot: string | null) => void
}

/** First run: connect the AIs and GitHub, pick projects, make the bot. */
export function Onboarding({ onFinish }: OnboardingProps) {
  const [step, setStep] = useState<Step>('welcome')
  const [setup, setSetup] = useState<SetupState | null>(null)
  const [github, setGithub] = useState<GitHubState | null>(null)
  const [selected, setSelected] = useState<string[]>([])
  const [design, setDesign] = useState<BotDesign>(() => designFrom(PRESETS[0], 'claude'))
  const [created, setCreated] = useState<Bot[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const loadSetup = useCallback(async () => {
    setLoading(true)
    try {
      const next = await fetchSetup()
      setSetup(next)
      // First load: start with the most recently used project and an AI that is signed in.
      setSelected((current) => (current.length > 0 || !next.projects[0] ? current : [next.projects[0].project]))
      const usable = next.ais.filter((ai) => ai.bots && ai.enabled && ai.logged_in === true)
      const ready = usable[0]
      if (ready) setDesign((current) => (usable.some((ai) => ai.id === current.agent) ? current : { ...current, agent: ready.id as BotAgent }))
    } catch (failure: unknown) {
      setError(errorText(failure))
    } finally {
      setLoading(false)
    }
  }, [])

  const loadGitHub = useCallback(async () => {
    setLoading(true)
    try {
      setGithub(await fetchGitHub())
    } catch (failure: unknown) {
      setError(errorText(failure))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadSetup()
  }, [loadSetup])
  useEffect(() => {
    if (step === 'github' && github === null) void loadGitHub()
  }, [step, github, loadGitHub])

  // A finished clone shows up as a local project; check it right away.
  const cloning = setup?.clones.running ?? []
  useEffect(() => {
    if (cloning.length === 0) return undefined
    const timer = window.setInterval(() => {
      fetchSetup()
        .then((next) => {
          setSetup(next)
          const finished = cloning.filter((repo) => !next.clones.running.includes(repo) && !next.clones.failed[repo])
          const fresh = next.projects.filter((p) => p.github && finished.includes(p.github)).map((p) => p.project)
          if (fresh.length > 0) setSelected((current) => [...new Set([...current, ...fresh])])
        })
        .catch((failure: unknown) => setError(errorText(failure)))
    }, CLONE_POLL_MS)
    return () => window.clearInterval(timer)
  }, [cloning])

  const go = (next: Step) => {
    setError(null)
    setStep(next)
  }
  const anyAi = setup?.ais.some((ai) => ai.bots && ai.enabled && ai.logged_in === true) ?? false

  const createBots = async () => {
    setLoading(true)
    setError(null)
    try {
      const persona = personaOf(design, o.defaultName)
      let bots: Bot[] = []
      for (const project of selected) {
        bots = (await botAction('activate', { project, agent: design.agent, persona })).bots
      }
      setCreated(bots.filter((bot) => bot.agent === design.agent && selected.some((project) => bot.dir.endsWith(`/${project}`))))
      go('done')
    } catch (failure: unknown) {
      setError(errorText(failure))
    } finally {
      setLoading(false)
    }
  }

  const finish = async (firstBot: string | null) => {
    try {
      await finishSetup()
      onFinish(firstBot)
    } catch (failure: unknown) {
      setError(errorText(failure))
    }
  }

  const face = faceOf(design.shape, design.color, design.name, design.finish)
  const mood: AvatarState = loading ? 'thinking' : step === 'done' ? 'happy' : 'idle'
  const titles: Record<Step, [string, string]> = {
    welcome: [o.welcomeTitle, o.welcomeText],
    ais: [o.aisTitle, o.aisText],
    github: [o.githubTitle, o.githubText],
    projects: [o.projectsTitle, o.projectsText],
    bot: [o.botTitle, o.botText],
    done: [o.doneTitle, created.length > 0 ? o.doneText.replace('{name}', personaOf(design, o.defaultName).name) : o.doneEmpty],
  }
  const [title, text] = titles[step]
  const index = ORDER.indexOf(step)

  return (
    <div className="boas-vindas">
      <header className="bv-topo">
        <span className="marca">
          <Mark />
          raiox
        </span>
        <ol className="bv-trilho" aria-label="setup">
          {o.steps.map((label, i) => (
            <li key={label} className={i + 1 < index ? 'feito' : i + 1 === index ? 'atual' : ''}>
              <i />
              <span>{`${String(i + 1).padStart(2, '0')} ${label}`}</span>
            </li>
          ))}
        </ol>
      </header>

      <div className="bv-corpo">
        <aside className="bv-palco">
          <h1>{title}</h1>
          <p>{text}</p>
          <div className="bv-luz" style={{ backgroundImage: `url(${stageLight(design.color)})` }}>
            <Avatar agent={face} size={window.matchMedia(PHONE).matches ? STAGE_SIZE_PHONE : STAGE_SIZE} state={mood} followPointer />
          </div>
          <span className="bv-legenda">{`${personaOf(design, o.defaultName).name} · ${AGENT_LABEL[design.agent]}`}</span>
        </aside>

        <section className="cartao bv-cartao">
          {step === 'welcome' && <StepWelcome onNext={() => go('ais')} />}
          {step === 'ais' && (
            <StepAis
              setup={setup}
              loading={loading}
              onCheck={() => void loadSetup()}
              onBack={() => go('welcome')}
              onNext={() => go(anyAi ? 'github' : 'done')}
            />
          )}
          {step === 'github' && setup && (
            <StepGitHub
              github={github}
              setup={setup}
              loading={loading}
              onCheck={() => void loadGitHub()}
              onBack={() => go('ais')}
              onNext={() => go('projects')}
            />
          )}
          {step === 'projects' && setup && (
            <StepProjects
              setup={setup}
              github={github}
              selected={selected}
              onSelect={setSelected}
              onSetup={setSetup}
              onError={setError}
              onBack={() => go('github')}
              onNext={() => go('bot')}
              onSkip={() => go('done')}
            />
          )}
          {step === 'bot' && setup && (
            <StepBot
              setup={setup}
              design={design}
              projects={selected}
              busy={loading}
              onDesign={setDesign}
              onBack={() => go('projects')}
              onCreate={() => void createBots()}
            />
          )}
          {step === 'done' && <StepDone bots={created} onFinish={(id) => void finish(id)} />}
          {error && <p className="nota failed bv-erro">{error}</p>}
        </section>
      </div>
    </div>
  )
}
