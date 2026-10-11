import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Client, Identity, IdentityRequest } from '../api/types'
import { IdentityPanel } from './IdentityPanel'

/**
 * A real `fetch` stub rather than a mocked `useIdentity` or `api/client`: the
 * bug this file guards against is a dirty-check that let a real edit go
 * unsaved, so the honest evidence is what actually crosses the wire -- the
 * exact ordered roles, the untouched realm and clients, and the `If-Match`
 * revision -- produced by the real hook and the real `putIdentity` serializer.
 */
function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

/** The roles the platform requires; `RoleEditor` offers no Remove for them. */
const REQUIRED = ['Client Realm Administrator', 'Client Realm User'] as const

function client(overrides: Partial<Client> = {}): Client {
  return { id: 'acme', displayName: 'Acme', hosts: [], realm: 'acme', revision: 'r1', ...overrides }
}

function identity(roles: readonly string[], overrides: Partial<Identity> = {}): Identity {
  return {
    realm: 'acme',
    roles,
    clients: [
      {
        id: 'web',
        type: 'oidc',
        pkce: 's256',
        redirect: { strategy: 'claimedHttps', uris: ['https://acme.example.com/callback'] },
      },
    ],
    apiVersion: 'fabric.fieldstate.nz/v2',
    revision: 'r1',
    reconciliation: { status: 'applied', observedAtUnix: null, detail: null },
    ...overrides,
  }
}

interface Put {
  readonly ifMatch: string | null
  readonly body: IdentityRequest
}

let puts: Put[]

/**
 * Serves `baseline` on GET, and answers a PUT with exactly what was written at
 * a new revision -- the way the control plane does, so a successful save
 * becomes the next clean baseline.
 */
function serve(baseline: Identity) {
  vi.stubGlobal(
    'fetch',
    vi.fn((_input: string, init?: RequestInit) => {
      if (init?.method !== 'PUT') {
        return Promise.resolve(jsonResponse(200, baseline))
      }
      // `putIdentity` serializes to a JSON string; anything else is a harness bug.
      if (typeof init.body !== 'string') {
        throw new Error('PUT body was not a JSON string')
      }
      const body = JSON.parse(init.body) as IdentityRequest
      puts.push({ ifMatch: new Headers(init.headers).get('If-Match'), body })
      return Promise.resolve(jsonResponse(200, { ...baseline, ...body, revision: 'r2' }))
    }),
  )
}

const save = () => screen.getByRole('button', { name: /Save/ })
const discard = () => screen.getByRole('button', { name: 'Discard' })

/**
 * The role names as rendered, top to bottom. Scoped to the editor's own list:
 * `ApplicationClients` below it renders list items too.
 */
function shownRoles(): string[] {
  const editor = screen.getByLabelText('Add a role').closest('.roles')
  if (editor === null) throw new Error('no role editor')
  return within(editor as HTMLElement)
    .getAllByRole('listitem')
    .map((item) => item.firstChild?.textContent ?? '')
}

/**
 * Resolves once the baseline roles are actually on screen. The heading paints
 * before the effect that copies `identity.value.roles` into editor state runs,
 * so waiting on the heading alone would let a test act on an empty editor.
 */
async function renderPanel(baseline: Identity) {
  serve(baseline)
  render(<IdentityPanel client={client()} />)
  await waitFor(() => {
    expect(shownRoles()).toEqual(baseline.roles)
  })
  return userEvent.setup()
}

async function remove(user: ReturnType<typeof userEvent.setup>, role: string) {
  const row = screen.getByText(role).closest('li')
  if (row === null) throw new Error(`no row for ${role}`)
  await user.click(within(row).getByRole('button', { name: 'Remove' }))
}

async function add(user: ReturnType<typeof userEvent.setup>, role: string) {
  await user.type(screen.getByLabelText('Add a role'), role)
  await user.click(screen.getByRole('button', { name: 'Add' }))
}

/** Waits for the write to settle and the panel to be clean again at the new revision. */
async function expectSaved(roles: readonly string[]) {
  await waitFor(() => {
    expect(screen.getByRole('button', { name: 'Save changes' })).toBeDisabled()
  })
  expect(puts).toHaveLength(1)
  expect(puts[0]).toEqual({
    ifMatch: '"r1"',
    body: { realm: 'acme', roles, clients: identity([]).clients },
  })
  expect(discard()).toBeDisabled()
  expect(shownRoles()).toEqual(roles)
}

beforeEach(() => {
  puts = []
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('IdentityPanel: a change in role names is a change, even when a join hides it', () => {
  it('saves splitting one role into two whose names join to the same text', async () => {
    // `['Billing Approver']` and `['Billing', 'Approver']` join on a space to
    // the same string. Different lengths, same text.
    const user = await renderPanel(identity([...REQUIRED, 'Billing Approver']))

    await remove(user, 'Billing Approver')
    await add(user, 'Billing')
    await add(user, 'Approver')

    expect(save()).toBeEnabled()
    expect(discard()).toBeEnabled()
    await user.click(save())

    await expectSaved([...REQUIRED, 'Billing', 'Approver'])
  })

  it('saves moving a word across a boundary between two same-length lists', async () => {
    // `['Billing Approver', 'Auditor']` and `['Billing', 'Approver Auditor']`
    // join to the same string and have the same length, so a length check
    // alone would not catch it either.
    const user = await renderPanel(identity([...REQUIRED, 'Billing Approver', 'Auditor']))

    await remove(user, 'Billing Approver')
    await remove(user, 'Auditor')
    await add(user, 'Billing')
    await add(user, 'Approver Auditor')

    expect(save()).toBeEnabled()
    expect(discard()).toBeEnabled()
    await user.click(save())

    await expectSaved([...REQUIRED, 'Billing', 'Approver Auditor'])
  })
})

describe('IdentityPanel: order is part of the document', () => {
  const ordered = [...REQUIRED, 'Invoicing Approver', 'Auditor']

  it('treats a reorder as a change and writes the new order', async () => {
    const user = await renderPanel(identity(ordered))

    await remove(user, 'Invoicing Approver')
    await add(user, 'Invoicing Approver')

    expect(save()).toBeEnabled()
    expect(discard()).toBeEnabled()
    await user.click(save())

    await expectSaved([...REQUIRED, 'Auditor', 'Invoicing Approver'])
  })

  it('is clean again when the original names return in their original order', async () => {
    const user = await renderPanel(identity(ordered))

    await remove(user, 'Auditor')
    expect(save()).toBeEnabled()

    await add(user, 'Auditor')

    expect(shownRoles()).toEqual(ordered)
    expect(save()).toBeDisabled()
    expect(discard()).toBeDisabled()
    expect(puts).toHaveLength(0)
  })

  it('discards back to the original names and order without a write', async () => {
    const user = await renderPanel(identity(ordered))

    await remove(user, 'Invoicing Approver')
    await add(user, 'Billing')
    await user.click(discard())

    expect(shownRoles()).toEqual(ordered)
    expect(save()).toBeDisabled()
    expect(discard()).toBeDisabled()
    expect(puts).toHaveLength(0)
  })
})

describe('IdentityPanel: nothing happens on its own', () => {
  it('starts clean, writes nothing, and keeps the required roles unremovable', async () => {
    await renderPanel(identity([...REQUIRED, 'Auditor']))

    expect(save()).toBeDisabled()
    expect(discard()).toBeDisabled()
    expect(puts).toHaveLength(0)
    expect(screen.getAllByText('required')).toHaveLength(2)
    expect(screen.getAllByRole('button', { name: 'Remove' })).toHaveLength(1)
  })
})
