import assert from 'node:assert/strict'
import test from 'node:test'
import { changedQueryKeys } from '../apps/desktop/src/data/changeKeys.ts'

const before = { epoch: 'old', domains: { contacts: 8, activity: 2, leads: 0, mail: 3, configuration: 1 } }

test('uses the previous revision to update exactly the affected views', () => {
  const customerWrite = { ...before, domains: { ...before.domains, contacts: 9 } }
  assert.deepEqual(changedQueryKeys(before, customerWrite), new Set(['contacts', 'contact', 'lead']))
  assert.deepEqual(changedQueryKeys(customerWrite, customerWrite), new Set())
  const activityWrite = { ...customerWrite, domains: { ...customerWrite.domains, activity: 3 } }
  assert.deepEqual(changedQueryKeys(customerWrite, activityWrite), new Set(['contacts', 'contact', 'activities', 'dashboard']))
})

test('refreshes mail on approval expiration and all profile views on restore', () => {
  const expired = { ...before, domains: { ...before.domains, mail_expiration: 1 } }
  assert.deepEqual(changedQueryKeys(before, expired), new Set(['mail-drafts', 'mail-draft', 'dashboard']))
  const restored = { ...before, epoch: 'restored' }
  const refreshed = changedQueryKeys(before, restored)
  for (const key of ['contacts', 'contact', 'activities', 'inbox', 'mail-draft', 'backups', 'crm-states']) {
    assert.ok(refreshed.has(key), key)
  }
})
