import assert from 'node:assert/strict'
import test from 'node:test'
import { testLocations } from '../src/components/chat/projectReference.ts'

test('converts compiler and traceback locations into project file references', () => {
  const result = testLocations([
    '\x1b[31m --> src/example.rs:6:3\x1b[0m',
    'src/example.rs:6:3',
    'src/demo.ts(7,2): error TS1000',
    '  File "C:/Example/My Project/test_demo.py", line 28',
    '  at example (C:/outside/test.ts:8:2)',
  ].join('\n'), 'C:/Example/My Project')
  assert.deepEqual(result, [
    { path: 'src/example.rs', line: 6 },
    { path: 'test_demo.py', line: 28 },
    { path: 'src/demo.ts', line: 7 },
  ])
})
