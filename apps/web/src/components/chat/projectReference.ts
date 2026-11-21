export interface ProjectReference {
  path: string
  line?: number
}

export interface ProjectFileLinks {
  root: string
  open: (file: ProjectReference) => void
}

function portable(path: string): string {
  return path.replaceAll('\\', '/').replace(/^\/\/\?\/UNC\//i, '//').replace(/^\/\/\?\/(?=[a-z]:\/)/i, '')
}

export function projectReference(target: string, root: string): ProjectReference | null {
  let path = portable(target.trim())
  if (/^file:\/\/\//i.test(path)) path = path.slice(7).replace(/^\/(?=[a-z]:\/)/i, '')
  const position = path.match(/(?:#L(\d+)(?:-L\d+)?|:(\d+)(?::\d+)?)$/i)
  if (position?.index !== undefined) path = path.slice(0, position.index)
  try {
    path = portable(decodeURIComponent(path))
  } catch {
    return null
  }
  if (/^(?:\/|[a-z]:\/)/i.test(path)) {
    const base = portable(root).replace(/\/+$/, '') + '/'
    const windows = /^(?:[a-z]:\/|\/\/)/i.test(base)
    if (!(windows ? path.toLowerCase().startsWith(base.toLowerCase()) : path.startsWith(base))) return null
    path = path.slice(base.length)
  }
  path = path.replace(/^\.\//, '')
  if (!position && !/(?:\/|\.[a-z\d]{1,12}$)/i.test(path)) return null
  if (!path || path.length > 512 || /[:<>\\\x00-\x1f]/.test(path) || path.split('/').some((part) => !part || part === '..' || part === '.')) return null
  const line = Number(position?.[1] ?? position?.[2])
  return { path, line: Number.isSafeInteger(line) && line > 0 ? line : undefined }
}

const ANSI = /\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-?]*[ -/]*[@-~]/g
const COLON = /((?:[A-Z]:)?(?:[^\s():<>/\\]*[/\\])*[\w.-]+\.[a-z\d]+):(\d+)(?::\d+)?/gi
const PAREN = /((?:[A-Z]:)?(?:[^\s():<>/\\]*[/\\])*[\w.-]+\.[a-z\d]+)\((\d+),\s*\d+\)/gi
const PYTHON = /\bFile "([^"\r\n]+)", line (\d+)/g
const COMPILER = /(?:^|\s)-->\s+(.+?):(\d+):\d+\s*$/gm

export function testLocations(output: string, root: string): ProjectReference[] {
  const text = output.replace(ANSI, '')
  const files = new Map<string, ProjectReference>()
  for (const pattern of [COMPILER, PYTHON, PAREN, COLON]) {
    for (const [, path, line] of text.matchAll(pattern)) {
      const file = projectReference(path + ':' + line, root)
      if (file?.line) files.set(file.path + ':' + file.line, file)
      if (files.size >= 40) return [...files.values()]
    }
  }
  return [...files.values()]
}
