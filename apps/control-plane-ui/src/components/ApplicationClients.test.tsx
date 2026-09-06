import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { V1_API_VERSION, type ApplicationClient, type RedirectStrategyName } from '../api/types'
import { ApplicationClients } from './ApplicationClients'

/** The `v2` schema version, used wherever a test does not care about `v1`. */
const V2_API_VERSION = 'fabric.fieldstate.nz/v2'

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
    render(<ApplicationClients apiVersion={V2_API_VERSION} clients={[client()]} />)

    expect(screen.getByText('development')).toBeInTheDocument()
    expect(screen.getByText('http://127.0.0.1/callback')).toBeInTheDocument()
  })

  it('shows PKCE S256 for a claimedHttps client', () => {
    render(
      <ApplicationClients
        apiVersion={V2_API_VERSION}
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
        apiVersion={V2_API_VERSION}
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

  it('renders a PKCE method this build does not recognise rather than crashing on it', () => {
    // The counterpart to the unknown-strategy case above: a future PKCE
    // method reaching an old console must still show up, uppercased for the
    // label the same way `s256` becomes `S256`, rather than being hidden or
    // guessed at. The cast is deliberate, for the same reason as above.
    const unknownPkce = 'futureMethod' as ApplicationClient['pkce']

    render(
      <ApplicationClients
        apiVersion={V2_API_VERSION}
        clients={[client({ id: 'future', pkce: unknownPkce })]}
      />,
    )

    expect(screen.getByText(/PKCE FUTUREMETHOD/)).toBeInTheDocument()
  })

  it('gives the strategy value its own class rather than letting CSS capitalise it', () => {
    // `.badge__status` capitalises by default, which would display
    // `claimedHttps` as `ClaimedHttps` -- a spelling the API refuses. RTL's
    // `textContent` already equals the wire spelling regardless of CSS,
    // because `text-transform` only changes what is painted, never the DOM
    // text node -- so a `getByText('claimedHttps')` assertion alone would
    // pass even if the capitalising class were still applied. Asserting the
    // class is what actually pins the fix.
    render(
      <ApplicationClients
        apiVersion={V2_API_VERSION}
        clients={[
          client({
            redirect: { strategy: 'claimedHttps', uris: ['https://www.example.com/callback'] },
          }),
        ]}
      />,
    )

    expect(screen.getByText('claimedHttps')).toHaveClass('badge__status--literal')
  })

  it('names the strategy and version badges rather than leaving them bare words', () => {
    render(<ApplicationClients apiVersion={V2_API_VERSION} clients={[client()]} />)

    expect(screen.getByText('redirect strategy')).toBeInTheDocument()
    expect(screen.getByText('document version')).toBeInTheDocument()
  })

  it('says nothing is declared when there are no application clients', () => {
    render(<ApplicationClients apiVersion={V2_API_VERSION} clients={[]} />)

    expect(screen.getByText(/no applications are declared/i)).toBeInTheDocument()
  })
})

describe("the document's schema version", () => {
  it('shows the version, with its own class so it is not capitalised either', () => {
    render(<ApplicationClients apiVersion={V1_API_VERSION} clients={[]} />)

    expect(screen.getByText(V1_API_VERSION)).toHaveClass('badge__status--literal')
  })

  it('says an edit will migrate a v1 document to v2', () => {
    render(<ApplicationClients apiVersion={V1_API_VERSION} clients={[]} />)

    expect(screen.getByText(/an edit will migrate this document to v2/i)).toBeInTheDocument()
  })

  it('says nothing about migration for a v2 document', () => {
    render(<ApplicationClients apiVersion={V2_API_VERSION} clients={[]} />)

    expect(screen.queryByText(/will migrate/i)).not.toBeInTheDocument()
  })
})
