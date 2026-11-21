import type { TrailItem } from '../../api'

// Only classify single commands. A pipeline or a chained command can hide the test's own exit status.
const compound = /[\r\n;&|<>]/
const testRunners = [
  /^(?:npm|pnpm|yarn)\s+(?:(?:--prefix|--dir|-C)\s+\S+\s+)?(?:run\s+)?test(?=\s|:|$)/i,
  /^(?:bun|go|dotnet|cargo(?:\s+\+\S+)?)\s+test(?=\s|$)/i,
  /^(?:python3?\s+-m\s+)?pytest(?=\s|$)/i,
]
const inspectionFlag = /(?:^|\s)(?:-h|--help|--version|--list|--listTests|--collect-only|--dry-run)(?=\s|$)/
const powershell = /^(?:"?[a-z]:[\\/][^"\r\n]*[\\/])?(?:powershell|pwsh)(?:\.exe)?"?\s+(?:-(?:NoProfile|NoLogo|NonInteractive)\s+)*-(?:Command|c)\s+/i
const readArgument = /(?:"([^"]+)"|'([^']+)'|([^\s"']+))/
const fileReads = [
  new RegExp(`^cat\\s+(?:--\\s+)?${readArgument.source}$`),
  new RegExp(`^Get-Content\\s+(?:(?:-LiteralPath|-Path)\\s+)?${readArgument.source}(?:\\s+-(?:TotalCount|Tail)\\s+\\d+)?$`, 'i'),
  new RegExp(`^sed\\s+-n\\s+['"]?\\d+(?:,\\d+)?p['"]?\\s+${readArgument.source}$`),
]

function commandBody(command: string): string {
  const text = command.trim()
  const prefix = powershell.exec(text)?.[0]
  return prefix ? text.slice(prefix.length).replace(/^"|"$/g, '') : text
}

export function isTestCommand(command: string): boolean {
  const text = commandBody(command)
  return !compound.test(text) && !inspectionFlag.test(text) && testRunners.some((runner) => runner.test(text))
}

/** Files and test commands with a reported outcome, independent of the agent's written summary. */
export function trailEvidence(items: TrailItem[]) {
  const files = new Set<string>()
  const tests: TrailItem[] = []
  for (const item of items) {
    if (!item.target || item.ok == null) continue
    if (item.kind === 'run' && isTestCommand(item.target)) tests.push({ ...item, target: commandBody(item.target) })
    if (!item.ok) continue
    if (item.kind === 'read') files.add(item.target)
    if (item.kind === 'run') {
      const text = commandBody(item.target)
      if (compound.test(text)) continue
      const match = fileReads.map((read) => read.exec(text)).find(Boolean)
      const file = match?.[1] ?? match?.[2] ?? match?.[3]
      if (file && !/^[-~]/.test(file) && !/[$%*?\x60[\]]/.test(file)) files.add(file)
    }
  }
  return { files: [...files], tests }
}
