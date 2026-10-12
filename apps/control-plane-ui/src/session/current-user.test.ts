import { describe, expect, it } from 'vitest'

import { readCurrentUser } from './current-user'

/** `COMPLETE` with one field left out entirely, not set to `null`. */
function without(field: keyof typeof COMPLETE): Record<string, unknown> {
  return Object.fromEntries(Object.entries(COMPLETE).filter(([name]) => name !== field))
}

const COMPLETE = {
  subject: 'f81d4fae-7dec-11d0-a765-00a0c91e6bf6',
  userName: 'brett',
  displayName: 'Brett Example',
  email: 'brett@example.test',
  roles: ['fabric-operator'],
}

describe('readCurrentUser', () => {
  it('reads a complete answer', () => {
    expect(readCurrentUser(COMPLETE)).toEqual(COMPLETE)
  })

  it('reads null username and email, which the contract allows', () => {
    const body = { ...COMPLETE, userName: null, email: null, displayName: COMPLETE.subject }

    expect(readCurrentUser(body)).toEqual(body)
  })

  it.each([
    ['nothing', null],
    ['a string', 'signed in'],
    ['the operator route answer', { subject: 'brett' }],
    ['no subject', { ...COMPLETE, subject: undefined }],
    ['a blank subject', { ...COMPLETE, subject: '  ' }],
    ['a blank display name', { ...COMPLETE, displayName: '' }],
    ['an omitted username', without('userName')],
    ['an omitted email', without('email')],
    ['a numeric email', { ...COMPLETE, email: 42 }],
    ['no roles', { ...COMPLETE, roles: [] }],
    ['roles that are not strings', { ...COMPLETE, roles: [1] }],
    ['roles that are not a list', { ...COMPLETE, roles: 'fabric-operator' }],
  ])('refuses %s', (_case, body) => {
    expect(readCurrentUser(body)).toBeNull()
  })
})
