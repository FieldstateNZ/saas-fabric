import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { ApplicationClient, RedirectStrategyName } from '../api/types'
import { ApplicationClients } from './ApplicationClients'

/** An application client to render, with a `development` loopback callback by default. */
function client(overrides: Partial<ApplicationClient> = {}): ApplicationClient {
  return {
    id: 'native',
    type: 'oidc',
    pkce: 's256',
    redirect: { strategy: 'development', uris: ['http://127.0.0.1/callback'] },
    ...overrides,
  }
}

describe('the application clients list', () => {
  it('renders a development client with its strategy badge and its callbacks', () => {
    render(<ApplicationClients clients={[client()]} />)

    expect(screen.getByText('development')).toBeInTheDocument()
    expect(screen.getByText('http://127.0.0.1/callback')).toBeInTheDocument()
  })

  it('shows PKCE S256 for a claimedHttps client', () => {
    render(
      <ApplicationClients
        clients={[
          client({
            id: 'web',
            redirect: { strategy: 'claimedHttps', uris: ['https://www.example.com/callback'] },
          }),
        ]}
      />,
    )

    expect(screen.getByText('claimedHttps')).toBeInTheDocument()
    expect(screen.getByText(/PKCE S256/)).toBeInTheDocument()
  })

  it('renders a strategy this build does not recognise rather than crashing on it', () => {
    // The UI states only what Fabric observed: a strategy the console has not
    // been taught about yet is not this build's to hide or to guess a
    // fallback for. The cast is deliberate -- the wire is not statically
    // typed, and a future strategy reaching an old console is exactly the
    // case this test pins.
    const unknownStrategy = 'quantumHandshake' as RedirectStrategyName

    render(
      <ApplicationClients
        clients={[
          client({
            id: 'future',
            redirect: { strategy: unknownStrategy, uris: ['https://future.example.com/callback'] },
          }),
        ]}
      />,
    )

    expect(screen.getByText('quantumHandshake')).toBeInTheDocument()
  })

  it('says nothing is declared when there are no application clients', () => {
    render(<ApplicationClients clients={[]} />)

    expect(screen.getByText(/no applications are declared/i)).toBeInTheDocument()
  })
})
