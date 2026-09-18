import { describe, expect, it } from 'vitest'

import type { DataSource } from '../api/data-source-types'
import { connectionLabel, draftFrom, isDeclarableConnection } from './data-source-draft'

const base: DataSource = {
  id: 'legacy-oracle-01',
  revision: 1,
  connector: 'oracle-legacy',
  connection: { kind: 'named', name: 'primary' },
  placement: 'dedicated',
  residency: { region: 'nz', jurisdiction: null },
  pool: { maxConnections: 5, idleTimeoutSeconds: 60, acquireTimeoutSeconds: 5 },
  capabilities: { writable: true, acceptsNewTenants: false },
  discriminator: null,
  labels: {},
}

describe('isDeclarableConnection', () => {
  it('accepts named', () => {
    expect(isDeclarableConnection({ kind: 'named', name: 'primary' })).toBe(true)
  })

  it('accepts secret', () => {
    expect(isDeclarableConnection({ kind: 'secret', reference: 'secret/data/x' })).toBe(true)
  })

  it('refuses a kind outside named/secret -- what a hand edit could put there', () => {
    expect(isDeclarableConnection({ kind: 'default' })).toBe(false)
  })
})

describe('connectionLabel', () => {
  it('describes a named connection', () => {
    expect(connectionLabel({ kind: 'named', name: 'primary' })).toBe('Named: primary')
  })

  it('describes a secret connection', () => {
    expect(connectionLabel({ kind: 'secret', reference: 'secret/data/x' })).toBe('Secret: secret/data/x')
  })

  it('never coerces an unknown kind into looking like a secret', () => {
    expect(connectionLabel({ kind: 'default' })).toBe('Not declarable')
  })
})

describe('draftFrom: connection honesty', () => {
  it('carries a named connection through unchanged', () => {
    const draft = draftFrom(base)
    expect(draft.connectionKind).toBe('named')
    expect(draft.connectionValue).toBe('primary')
  })

  it('falls back to an empty named connection for a kind it cannot represent, rather than throwing', () => {
    const draft = draftFrom({ ...base, connection: { kind: 'default' } })
    expect(draft.connectionKind).toBe('named')
    expect(draft.connectionValue).toBe('')
  })
})
