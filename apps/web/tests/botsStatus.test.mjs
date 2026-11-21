import assert from 'node:assert/strict'
import test from 'node:test'
import { mergeBotsStatus } from '../src/botsStatus.ts'

const messages = [{ kind: 'ask' }]
const side = [{ id: 'side', messages }]
const state = {
  chat_revision: 10,
  bots: [{ id: 'a', busy: true, busy_thread: 'side', started_ms: 100, messages, threads: side }],
  channels: [{ id: 'channel', working: true, messages }],
}
const status = {
  chat_revision: 10,
  bots: [{ id: 'a', busy: true, busy_thread: 'side', started_ms: 100, step: { kind: 'read' } }],
  channels: [{ id: 'channel', working: true }],
}

test('a live poll updates the step and keeps loaded histories', () => {
  const merged = mergeBotsStatus(state, status)
  assert.deepEqual(merged.bots[0].step, { kind: 'read' })
  assert.equal(merged.bots[0].messages, messages)
  assert.equal(merged.bots[0].threads, side)
  assert.equal(merged.channels[0], state.channels[0])
})

test('new messages, completed turns and changed device visibility reload the full snapshot', () => {
  assert.equal(mergeBotsStatus(state, { ...status, chat_revision: 11 }), null)
  assert.equal(mergeBotsStatus(state, { ...status, bots: [{ ...status.bots[0], busy: false }] }), null)
  assert.equal(mergeBotsStatus(state, { ...status, bots: [{ ...status.bots[0], started_ms: 101 }] }), null)
  assert.equal(mergeBotsStatus(state, { ...status, bots: [{ ...status.bots[0], id: 'other' }] }), null)
  assert.equal(mergeBotsStatus(state, { ...status, channels: [{ id: 'channel', working: false }] }), null)
})

test("finishing a parallel turn reloads while the first turn stays active", () => {
  const primary = { thread: "side", started_ms: 100 }
  const others = { thread: "main", started_ms: 200 }
  const previous = { ...state, bots: [{ ...state.bots[0], busy_threads: [primary, others] }] }
  const polled = { ...status, bots: [{ ...status.bots[0], busy_threads: [primary] }] }
  assert.equal(mergeBotsStatus(previous, polled), null)
})
