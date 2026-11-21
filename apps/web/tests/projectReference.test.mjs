import assert from 'node:assert/strict'
import test from 'node:test'
import { projectReference } from '../src/components/chat/projectReference.ts'

test('opens relative and project-local absolute references at the requested line', () => {
  assert.deepEqual(projectReference('src/app.ts:28:9', '/workspace/demo'), { path: 'src/app.ts', line: 28 })
  assert.deepEqual(projectReference('file:///C:/Example/My%20Project/src/main%20window.ts#L7', '\\\\?\\c:\\Example\\My Project'), { path: 'src/main window.ts', line: 7 })
  assert.deepEqual(projectReference('/workspace/demo/src/app.ts', '/workspace/demo'), { path: 'src/app.ts', line: undefined })
})

test('does not resolve external, escaping, or unrelated absolute references', () => {
  for (const target of ['https://example.com/test.ts', '../test.ts', 'src/%2e%2e/test.ts', '/workspace/demo-other/test.ts', 'sandbox:/mnt/data/app.ts']) {
    assert.equal(projectReference(target, '/workspace/demo'), null)
  }
})
