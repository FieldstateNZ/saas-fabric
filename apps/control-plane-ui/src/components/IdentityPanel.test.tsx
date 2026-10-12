import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { SAVED_INTENT } from '../api/lifecycle.fixture'
import type { Client, Identity, ProjectionCheck } from '../api/types'
import type { IdentityEditor } from '../hooks/useIdentity'
import { IdentityPanel } from './IdentityPanel'

const editor = vi.hoisted(() => ({ current: null as IdentityEditor | null }))

vi.mock('../hooks/useIdentity', () => ({
  useIdentity: () => editor.current,
}))

const client: Client = {
  id: 'newco',
  displayName: 'Newco',
  hosts: [],
  realm: 'newco',
  revision: 'r1',
  lifecycle: SAVED_INTENT,
  projection: { state: 'consistent' },
}

/** A hand-edited identity whose stored projection is `projection`. */
function handEdited(projection: ProjectionCheck): Identity {
  return {
    realm: 'newco',
    roles: [],
    clients: [],
    apiVersion: 'fabric.fieldstate.nz/v2',
    revision: 'r1',
    reconciliation: {
      status: 'handEdited',
      observedAtUnix: null,
      detail: null,
      handEdits: [{ client: 'analytics', field: 'clientDisabled' }],
      withheld: [],
    },
    projection,
  }
}

function showing(identity: Identity) {
  editor.current = {
    value: identity,
    loading: false,
    error: null,
    saving: false,
    save: vi.fn(),
    reassert: vi.fn(),
  }
  render(<IdentityPanel client={client} />)
}

describe('the identity panel', () => {
  it('offers re-assertion over a hand edit when the stored projection is consistent', () => {
    showing(handEdited({ state: 'consistent' }))

    expect(screen.getByRole('button', { name: /re-assert configuration/i })).toBeInTheDocument()
  })

  it('reports a diverged stored projection and does not offer re-assertion onto it', () => {
    showing(handEdited({ state: 'diverged', divergences: [{ client: 'analytics', field: 'redirectUris' }] }))

    expect(screen.getByText(/has a different set of callbacks/i)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /re-assert configuration/i })).not.toBeInTheDocument()
  })
})
