import assert from 'node:assert/strict'
import test from 'node:test'
import { isTestCommand, trailEvidence } from '../src/components/chat/trailEvidence.ts'

test('shows evidence from completed file reads and test commands', () => {
  const items = [
    { kind: 'read', target: 'README.md', ok: true },
    { kind: 'read', target: 'README.md', ok: true },
    { kind: 'read', target: 'unavailable.txt', ok: false },
    { kind: 'read', target: 'still-reading.txt' },
    { kind: 'run', target: "Get-Content -LiteralPath 'src/my project.ts'", ok: true },
    { kind: 'run', target: 'Get-Content $path', ok: true },
    { kind: 'run', target: 'cargo test --release', ok: false },
    { kind: 'run', target: 'cargo test --release', ok: true },
    { kind: 'run', target: 'npm test' },
    { kind: 'say', target: 'All tests passed.' },
  ]
  const { files, tests } = trailEvidence(items)
  assert.deepEqual(files, ['README.md', 'src/my project.ts'])
  assert.deepEqual(tests, [items[6], items[7]])
})

test('uses individual test commands with a result that can be attributed to them', () => {
  assert.equal(isTestCommand('npm run test:unit'), true)
  assert.equal(isTestCommand('pnpm --dir apps/web test'), true)
  assert.equal(isTestCommand('cargo +stable test --release'), true)
  assert.equal(isTestCommand('python -m pytest tests'), true)
  assert.equal(isTestCommand('cat README.md'), false)
  assert.equal(isTestCommand('npm test; git status'), false)
  assert.equal(isTestCommand('cargo test -- --list'), false)
  assert.equal(isTestCommand('npm test | tee test.log'), false)
})

test('recognizes straightforward reads and test runs from Codex command events', () => {
  const shell = String.raw`C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe" -Command "`
  const { files, tests } = trailEvidence([
    { kind: 'run', target: shell + 'Get-Content README.md -TotalCount 60', ok: true },
    { kind: 'run', target: "sed -n '1,80p' package.json", ok: true },
    { kind: 'run', target: shell + 'npm test', ok: true },
    { kind: 'run', target: 'cat [ab].txt', ok: true },
  ])
  assert.deepEqual(files, ['README.md', 'package.json'])
  assert.deepEqual(tests.map((item) => item.target), ['npm test'])
})
