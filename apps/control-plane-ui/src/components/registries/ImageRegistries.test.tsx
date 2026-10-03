/**
 * What the Image registries section says, and what it sends.
 *
 * The distinctions under test are the ADR's (0026 section 5): what was
 * proven, and when, rather than "connected"; the deployment's registry
 * listed as the deployment's; a credential shown by its account and who
 * set it, never by its token; and every refusal shown in the control
 * plane's own words.
 */
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { Registry, RegistryCredential, RegistryListing } from '../../api/registry-types'
import { ImageRegistries } from './ImageRegistries'
import { when } from './registry-words'

const TOKEN = 'ghp_ThisIsTheTokenAndItMustNeverShow'

const HELD: RegistryCredential = {
  username: 'fabric-reader',
  setBy: 'brett',
  setAt: 1_790_000_000,
  refused: false,
  unreadable: false,
}

const GHCR: Registry = {
  host: 'ghcr.io',
  kind: 'ghcr',
  endpoint: 'https://ghcr.io',
  realmOrigin: 'https://ghcr.io',
  deployment: true,
  installed: true,
  credential: HELD,
  registeredBy: 'brett',
  registeredAt: 1_789_000_000,
  repositories: [{ repository: 'ghcr.io/fieldstatenz/saas-fabric', provenAt: 1_790_100_000 }],
}

const DOCKER_HUB: Registry = {
  host: 'docker.io',
  kind: 'dockerHub',
  endpoint: 'https://registry-1.docker.io',
  realmOrigin: 'https://auth.docker.io',
  deployment: false,
  installed: true,
  credential: null,
  registeredBy: 'alice',
  registeredAt: 1_789_500_000,
  repositories: [],
}

const DISTRIBUTION: Registry = {
  host: 'registry.example.com:5000',
  kind: 'distribution',
  endpoint: 'https://registry.example.com:5000',
  realmOrigin: null,
  deployment: false,
  installed: true,
  credential: null,
  registeredBy: 'alice',
  registeredAt: 1_789_600_000,
  repositories: [],
}

function listingOf(...registries: Registry[]): RegistryListing {
  return { registries, deployment: { host: 'ghcr.io', endpoint: 'https://ghcr.io' } }
}

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

function refusal(status: number, code: string, message: string): Response {
  return jsonResponse(status, { error: { code, message } })
}

interface Call {
  readonly method: string
  readonly url: string
  readonly body: unknown
}

/**
 * A control plane that answers the listing with whatever `state.listing`
 * holds when it is asked, and every other call with `change`.
 */
function serve(
  state: { listing: RegistryListing },
  change: (call: Call) => Response = () => refusal(500, 'unexpected', 'not in this test'),
): Call[] {
  const calls: Call[] = []

  vi.stubGlobal(
    'fetch',
    vi.fn((url: string, init?: RequestInit) => {
      const call: Call = {
        method: init?.method ?? 'GET',
        url,
        body: typeof init?.body === 'string' ? JSON.parse(init.body) : undefined,
      }
      calls.push(call)

      if (call.method === 'GET' && url === '/api/integrations/registries') {
        return Promise.resolve(jsonResponse(200, state.listing))
      }

      return Promise.resolve(change(call))
    }),
  )

  return calls
}

/** One registry's card, so assertions cannot land on another. */
async function card(name: string) {
  return within(await screen.findByRole('region', { name }))
}

/** The token appears nowhere on the page: not as text, not as a value, not in markup. */
function expectNoToken(): void {
  expect(document.body.innerHTML).not.toContain(TOKEN)
  expect(document.body.textContent).not.toContain(TOKEN)
  expect(screen.queryByDisplayValue(TOKEN)).not.toBeInTheDocument()
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the image registries section: what it shows', () => {
  it('lists the deployment’s registry as the deployment’s, read anonymously', async () => {
    serve({ listing: listingOf() })
    render(<ImageRegistries />)

    const deployment = await card('The deployment’s registry')

    expect(deployment.getByText('ghcr.io')).toBeInTheDocument()
    expect(deployment.getByText('https://ghcr.io')).toBeInTheDocument()
    expect(deployment.getByText('This deployment’s configuration')).toBeInTheDocument()
    expect(deployment.getByText('None of its own. Read anonymously.')).toBeInTheDocument()
    // Configuration, not a registration: nothing to change or remove here.
    expect(deployment.queryByRole('button')).not.toBeInTheDocument()
  })

  it('shows no deployment’s registry for a deployment that manages no platform', async () => {
    serve({ listing: { registries: [], deployment: null } })
    render(<ImageRegistries />)

    expect(await screen.findByText('No registries are registered.')).toBeInTheDocument()
    expect(screen.queryByRole('region', { name: 'The deployment’s registry' })).not.toBeInTheDocument()
  })

  it('shows what was proven of a registry: kind, host, endpoint, realm, credential and repositories', async () => {
    serve({ listing: listingOf(GHCR) })
    render(<ImageRegistries />)

    const ghcr = await card('ghcr.io')

    expect(ghcr.getByText('GitHub Container Registry (ghcr)')).toBeInTheDocument()
    expect(ghcr.getAllByText('https://ghcr.io')).toHaveLength(2) // endpoint and realm origin
    expect(ghcr.getByText(`by brett, ${when(GHCR.registeredAt)}`)).toBeInTheDocument()
    expect(ghcr.getByText('fabric-reader')).toBeInTheDocument()
    expect(ghcr.getByText(`by brett, ${when(1_790_000_000)}`)).toBeInTheDocument()
    expect(ghcr.getByText('ghcr.io/fieldstatenz/saas-fabric')).toBeInTheDocument()
    expect(ghcr.getByText(`proven ${when(1_790_100_000)}`)).toBeInTheDocument()
    expect(ghcr.getByText(/Presented only for the repositories registered below/)).toBeInTheDocument()
  })

  it('says which registration is the deployment’s host, on both cards', async () => {
    serve({ listing: listingOf(GHCR) })
    render(<ImageRegistries />)

    expect((await card('ghcr.io')).getByText(/The deployment’s host/)).toBeInTheDocument()
    expect(
      (await card('The deployment’s registry')).getByText(/ghcr.io is also registered below/),
    ).toBeInTheDocument()
  })

  it('says a registry with no credential is read anonymously, and one that named no realm named none', async () => {
    serve({ listing: listingOf(DISTRIBUTION) })
    render(<ImageRegistries />)

    const distribution = await card('registry.example.com:5000')

    expect(distribution.getByText('Distribution API (distribution)')).toBeInTheDocument()
    expect(distribution.getByText('None named when it was proven')).toBeInTheDocument()
    expect(distribution.getByText('None. This registry is read anonymously.')).toBeInTheDocument()
    expect(distribution.getByText('None registered.')).toBeInTheDocument()
    expect(distribution.getByRole('button', { name: 'Set credential' })).toBeInTheDocument()
    expect(distribution.queryByRole('button', { name: 'Remove credential' })).not.toBeInTheDocument()
  })

  it('says a credential its realm refused is refused, and not presented', async () => {
    serve({
      listing: listingOf({ ...GHCR, credential: { ...HELD, refused: true } }),
    })
    render(<ImageRegistries />)

    const ghcr = await card('ghcr.io')

    expect(ghcr.getByText(/Refused by its realm/)).toBeInTheDocument()
    expect(ghcr.queryByText(/Presented only for/)).not.toBeInTheDocument()
  })

  it('says a credential that could not be read at the last start leaves the registry anonymous', async () => {
    serve({
      listing: listingOf({ ...GHCR, credential: { ...HELD, unreadable: true } }),
    })
    render(<ImageRegistries />)

    expect(
      (await card('ghcr.io')).getByText(/read anonymously until the credential is set again/),
    ).toBeInTheDocument()
  })

  it('says a registry the last start could not rebuild is not being read through', async () => {
    serve({ listing: listingOf({ ...DOCKER_HUB, installed: false }) })
    render(<ImageRegistries />)

    expect(
      (await card('docker.io')).getByText(/Nothing is read through this registry/),
    ).toBeInTheDocument()
  })

  it('never says connected, whatever state a registry is in', async () => {
    serve({
      listing: listingOf(
        GHCR,
        { ...DOCKER_HUB, installed: false },
        { ...DISTRIBUTION, credential: { ...HELD, refused: true } },
      ),
    })
    render(<ImageRegistries />)

    const section = await screen.findByRole('region', { name: 'Image registries' })
    await card('docker.io')

    expect(section.textContent).not.toMatch(/connect/i)
  })

  it('says it is loading, and then what it found', async () => {
    serve({ listing: listingOf(DOCKER_HUB) })
    render(<ImageRegistries />)

    expect(screen.getByText('Loading registries…')).toBeInTheDocument()
    expect(await card('docker.io')).toBeTruthy()
    expect(screen.queryByText('Loading registries…')).not.toBeInTheDocument()
  })

  it('shows a listing that could not be read as the server’s refusal, not as an empty list', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() =>
        Promise.resolve(refusal(503, 'registries_unavailable', 'the registry store is unavailable')),
      ),
    )
    render(<ImageRegistries />)

    const alert = await screen.findByRole('alert')

    expect(alert).toHaveTextContent('the registry store is unavailable')
    expect(alert).toHaveTextContent('Try again shortly.')
    expect(screen.queryByText('No registries are registered.')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Register a registry' })).not.toBeInTheDocument()
  })
})

describe('the image registries section: registering', () => {
  it('registers GHCR with its kind alone: no endpoint field, no host, no empty credential', async () => {
    const state = { listing: listingOf() }
    const calls = serve(state, () => jsonResponse(201, { ...GHCR, deployment: false }))
    const user = userEvent.setup()
    render(<ImageRegistries />)

    await user.click(await screen.findByRole('button', { name: 'Register a registry' }))
    expect(screen.queryByLabelText(/^endpoint/i)).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Register' }))

    expect(await card('ghcr.io')).toBeTruthy()
    expect(calls.filter((call) => call.method === 'POST')).toEqual([
      { method: 'POST', url: '/api/integrations/registries', body: { kind: 'ghcr' } },
    ])
    // The form closes once the registry is recorded.
    expect(screen.queryByRole('form', { name: 'Register a registry' })).not.toBeInTheDocument()
  })

  it('asks a distribution registry for its endpoint, sends the credential once, and never shows the token', async () => {
    const calls = serve({ listing: listingOf() }, () =>
      jsonResponse(201, {
        ...DISTRIBUTION,
        credential: { ...HELD, username: 'robot' },
      }),
    )
    const user = userEvent.setup()
    render(<ImageRegistries />)

    await user.click(await screen.findByRole('button', { name: 'Register a registry' }))
    await user.selectOptions(screen.getByRole('combobox', { name: 'Kind' }), 'distribution')
    await user.type(screen.getByLabelText(/^endpoint/i), 'https://registry.example.com:5000')
    const username = screen.getByLabelText(/^username/i)
    // Not this site's sign-in form: nothing the browser saved for the
    // console is filled in, and so nothing of it goes to a registry.
    expect(username).toHaveAttribute('autocomplete', 'off')
    await user.type(username, 'robot')
    const token = screen.getByLabelText(/^token/i)
    expect(token).toHaveAttribute('type', 'password')
    expect(token).toHaveAttribute('autocomplete', 'new-password')
    await user.type(token, TOKEN)
    await user.click(screen.getByRole('button', { name: 'Register' }))

    const registered = await card('registry.example.com:5000')
    expect(registered.getByText('robot')).toBeInTheDocument()
    expect(calls.filter((call) => call.method === 'POST')).toEqual([
      {
        method: 'POST',
        url: '/api/integrations/registries',
        body: {
          kind: 'distribution',
          endpoint: 'https://registry.example.com:5000',
          username: 'robot',
          token: TOKEN,
        },
      },
    ])
    expectNoToken()
  })

  it('shows a registration that did not prove in the server’s words, and empties the token', async () => {
    serve({ listing: listingOf() }, () =>
      refusal(422, 'registry_not_proven', 'the registry was not proven: /v2/ answered 404'),
    )
    const user = userEvent.setup()
    render(<ImageRegistries />)

    await user.click(await screen.findByRole('button', { name: 'Register a registry' }))
    await user.selectOptions(screen.getByRole('combobox', { name: 'Kind' }), 'distribution')
    await user.type(screen.getByLabelText(/^endpoint/i), 'https://registry.example.com')
    await user.type(screen.getByLabelText(/^username/i), 'robot')
    await user.type(screen.getByLabelText(/^token/i), TOKEN)
    await user.click(screen.getByRole('button', { name: 'Register' }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('the registry was not proven: /v2/ answered 404')
    expect(alert).toHaveTextContent('Nothing was recorded.')
    // The form stays, with what was typed -- except the token.
    expect(screen.getByLabelText(/^endpoint/i)).toHaveValue('https://registry.example.com')
    expect(screen.getByLabelText(/^username/i)).toHaveValue('robot')
    expect(screen.getByLabelText(/^token/i)).toHaveValue('')
    expectNoToken()
  })

  it('shows a rule the server refused by its message', async () => {
    serve({ listing: listingOf() }, () =>
      refusal(400, 'registry_invalid', 'a username is given with a token or not at all'),
    )
    const user = userEvent.setup()
    render(<ImageRegistries />)

    await user.click(await screen.findByRole('button', { name: 'Register a registry' }))
    await user.type(screen.getByLabelText(/^username/i), 'robot')
    await user.click(screen.getByRole('button', { name: 'Register' }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'a username is given with a token or not at all',
    )
  })
})

describe('the image registries section: repositories', () => {
  it('adds a repository typed in full, under its registry’s route, and shows when it was proven', async () => {
    const calls = serve({ listing: listingOf(DOCKER_HUB) }, () =>
      jsonResponse(200, {
        ...DOCKER_HUB,
        repositories: [{ repository: 'docker.io/library/nginx', provenAt: 1_790_200_000 }],
      }),
    )
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const docker = await card('docker.io')
    expect(docker.getByText(/starting with docker\.io\/ — for example docker\.io\/library\/nginx/)).toBeInTheDocument()
    await user.type(docker.getByLabelText(/^repository/i), 'docker.io/library/nginx')
    await user.click(docker.getByRole('button', { name: 'Add repository' }))

    expect(await docker.findByText('docker.io/library/nginx')).toBeInTheDocument()
    expect(docker.getByText(`proven ${when(1_790_200_000)}`)).toBeInTheDocument()
    expect(calls.filter((call) => call.method === 'PUT').map((call) => call.url)).toEqual([
      '/api/integrations/registries/docker.io/repositories/entry/library/nginx',
    ])
    expect(docker.getByLabelText(/^repository/i)).toHaveValue('')
  })

  it('refuses a repository that is not this registry’s before sending anything', async () => {
    const calls = serve({ listing: listingOf(DOCKER_HUB) })
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const docker = await card('docker.io')
    await user.type(docker.getByLabelText(/^repository/i), 'ghcr.io/fieldstatenz/saas-fabric')
    await user.click(docker.getByRole('button', { name: 'Add repository' }))

    expect(docker.getByRole('alert')).toHaveTextContent('starting with docker.io/')
    expect(calls.filter((call) => call.method !== 'GET')).toEqual([])
  })

  it('shows a repository that is not readable in the server’s words', async () => {
    serve({ listing: listingOf(GHCR) }, () =>
      refusal(
        422,
        'repository_not_readable',
        'ghcr.io/fieldstatenz/private is not readable through this registry',
      ),
    )
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const ghcr = await card('ghcr.io')
    await user.type(ghcr.getByLabelText(/^repository/i), 'ghcr.io/fieldstatenz/private')
    await user.click(ghcr.getByRole('button', { name: 'Add repository' }))

    const alert = await ghcr.findByRole('alert')
    expect(alert).toHaveTextContent('ghcr.io/fieldstatenz/private is not readable through this registry')
    expect(alert).toHaveTextContent('Nothing was recorded.')
    expect(ghcr.queryByText(/proven.*private/)).not.toBeInTheDocument()
  })

  it('removes a repository through its own route', async () => {
    const calls = serve({ listing: listingOf(GHCR) }, () =>
      jsonResponse(200, { ...GHCR, repositories: [] }),
    )
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const ghcr = await card('ghcr.io')
    await user.click(ghcr.getByRole('button', { name: 'Remove ghcr.io/fieldstatenz/saas-fabric' }))

    expect(await ghcr.findByText('None registered.')).toBeInTheDocument()
    expect(calls.filter((call) => call.method === 'DELETE').map((call) => call.url)).toEqual([
      '/api/integrations/registries/ghcr.io/repositories/entry/fieldstatenz/saas-fabric',
    ])
  })
})

describe('the image registries section: the credential', () => {
  it('sets a credential, shows its account and who set it, and never the token', async () => {
    const calls = serve({ listing: listingOf(DOCKER_HUB) }, () =>
      jsonResponse(200, {
        ...DOCKER_HUB,
        credential: { ...HELD, username: 'hub-reader', setBy: 'alice' },
      }),
    )
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const docker = await card('docker.io')
    await user.click(docker.getByRole('button', { name: 'Set credential' }))
    await user.type(docker.getByLabelText(/^username/i), 'hub-reader')
    const token = docker.getByLabelText(/^token/i)
    expect(token).toHaveAttribute('type', 'password')
    await user.type(token, TOKEN)
    await user.click(docker.getByRole('button', { name: 'Save credential' }))

    expect(await docker.findByText('hub-reader')).toBeInTheDocument()
    expect(docker.getByText(`by alice, ${when(1_790_000_000)}`)).toBeInTheDocument()
    expect(docker.getByRole('button', { name: 'Replace credential' })).toBeInTheDocument()
    expect(calls.filter((call) => call.method === 'PUT')).toEqual([
      {
        method: 'PUT',
        url: '/api/integrations/registries/docker.io/credential',
        body: { username: 'hub-reader', token: TOKEN },
      },
    ])
    expectNoToken()
  })

  it('shows a replacement its realm refused, empties the token, and keeps the held credential unmarked', async () => {
    const state = { listing: listingOf(GHCR) }
    const calls = serve(state, () =>
      // The replacement was refused; the credential still held was not.
      refusal(502, 'registry_refused', 'proving: the realm refused this registry’s credential'),
    )
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const ghcr = await card('ghcr.io')
    await user.click(ghcr.getByRole('button', { name: 'Replace credential' }))
    await user.type(ghcr.getByLabelText(/^username/i), 'fabric-reader')
    await user.type(ghcr.getByLabelText(/^token/i), TOKEN)
    await user.click(ghcr.getByRole('button', { name: 'Save credential' }))

    const alert = await ghcr.findByRole('alert')
    expect(alert).toHaveTextContent('proving: the realm refused this registry’s credential')
    expect(alert).toHaveTextContent('Nothing was recorded.')
    await waitFor(() => {
      expect(calls.filter((call) => call.method === 'GET')).toHaveLength(2)
    })
    expect(ghcr.queryByText(/Refused by its realm/)).not.toBeInTheDocument()
    expect(ghcr.getByLabelText(/^token/i)).toHaveValue('')
    expectNoToken()
  })

  it('shows the held credential refused once its realm refused it while a repository was proven', async () => {
    const state = { listing: listingOf(GHCR) }
    serve(state, () => {
      // The held credential was presented and refused: the server marked it.
      state.listing = listingOf({ ...GHCR, credential: { ...HELD, refused: true } })
      return refusal(502, 'registry_refused', 'proving a repository: the realm refused this registry’s credential')
    })
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const ghcr = await card('ghcr.io')
    await user.type(ghcr.getByLabelText(/^repository/i), 'ghcr.io/fieldstatenz/gone')
    await user.click(ghcr.getByRole('button', { name: 'Add repository' }))

    expect(await ghcr.findByRole('alert')).toHaveTextContent('Nothing was recorded.')
    expect(await ghcr.findByText(/Refused by its realm/)).toBeInTheDocument()
  })

  it('says a change the control plane did not answer in time may still be finishing', async () => {
    serve({ listing: listingOf(GHCR) }, () => new Response(null, { status: 504 }))
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const ghcr = await card('ghcr.io')
    await user.type(ghcr.getByLabelText(/^repository/i), 'ghcr.io/fieldstatenz/slow')
    await user.click(ghcr.getByRole('button', { name: 'Add repository' }))

    const alert = await ghcr.findByRole('alert')
    expect(alert).toHaveTextContent('may still be finishing this change')
    expect(alert).not.toHaveTextContent('504')
  })

  it('removes a credential, after which the registry is read anonymously', async () => {
    const calls = serve({ listing: listingOf(GHCR) }, () =>
      jsonResponse(200, { ...GHCR, credential: null }),
    )
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const ghcr = await card('ghcr.io')
    await user.click(ghcr.getByRole('button', { name: 'Remove credential' }))

    expect(await ghcr.findByText('None. This registry is read anonymously.')).toBeInTheDocument()
    expect(calls.filter((call) => call.method === 'DELETE').map((call) => call.url)).toEqual([
      '/api/integrations/registries/ghcr.io/credential',
    ])
  })
})

describe('the image registries section: a registry nothing is read through', () => {
  const STRANDED: Registry = { ...GHCR, installed: false }

  it('lists what it holds, offers only withdrawing its credential or removing it, and says so on the deployment’s card', async () => {
    serve({ listing: listingOf(STRANDED) })
    render(<ImageRegistries />)

    const ghcr = await card('ghcr.io')
    expect(ghcr.getByText(/Nothing is read through this registry/)).toBeInTheDocument()
    expect(ghcr.getByText('ghcr.io/fieldstatenz/saas-fabric')).toBeInTheDocument()
    expect(ghcr.queryByRole('button', { name: 'Replace credential' })).not.toBeInTheDocument()
    expect(ghcr.queryByRole('button', { name: 'Add repository' })).not.toBeInTheDocument()
    expect(
      ghcr.queryByRole('button', { name: 'Remove ghcr.io/fieldstatenz/saas-fabric' }),
    ).not.toBeInTheDocument()
    expect(ghcr.getByRole('button', { name: 'Remove credential' })).toBeInTheDocument()
    expect(ghcr.getByRole('button', { name: 'Remove ghcr.io…' })).toBeInTheDocument()

    const deployment = await card('The deployment’s registry')
    expect(deployment.getByText(/nothing is read through that registration/)).toBeInTheDocument()
    expect(deployment.queryByText(/is presented only/)).not.toBeInTheDocument()
  })
})

describe('the image registries section: removing a registry', () => {
  it('asks first, and keeps the registry when the operator keeps it', async () => {
    const calls = serve({ listing: listingOf(DISTRIBUTION) })
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const distribution = await card('registry.example.com:5000')
    await user.click(distribution.getByRole('button', { name: 'Remove registry.example.com:5000…' }))

    expect(distribution.getByText(/Remove registry.example.com:5000\? Its registered repositories go with it./)).toBeInTheDocument()
    await user.click(distribution.getByRole('button', { name: 'Keep it' }))

    expect(distribution.getByRole('button', { name: 'Remove registry.example.com:5000…' })).toBeInTheDocument()
    expect(calls.filter((call) => call.method !== 'GET')).toEqual([])
  })

  it('removes it once confirmed, naming the host with its port as one segment', async () => {
    const calls = serve({ listing: listingOf(GHCR, DISTRIBUTION) }, () => new Response(null, { status: 204 }))
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const distribution = await card('registry.example.com:5000')
    await user.click(distribution.getByRole('button', { name: 'Remove registry.example.com:5000…' }))
    await user.click(distribution.getByRole('button', { name: 'Remove registry' }))

    await waitFor(() => {
      expect(screen.queryByRole('region', { name: 'registry.example.com:5000' })).not.toBeInTheDocument()
    })
    expect(screen.getByRole('region', { name: 'ghcr.io' })).toBeInTheDocument()
    expect(calls.filter((call) => call.method === 'DELETE').map((call) => call.url)).toEqual([
      '/api/integrations/registries/registry.example.com%3A5000',
    ])
  })

  it('says a credential goes with it, when one is held', async () => {
    serve({ listing: listingOf(GHCR) })
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const ghcr = await card('ghcr.io')
    await user.click(ghcr.getByRole('button', { name: 'Remove ghcr.io…' }))

    expect(ghcr.getByText(/Its credential and its registered repositories go with it/)).toBeInTheDocument()
  })

  it('shows a removal the server refused, and keeps the registry', async () => {
    serve({ listing: listingOf(GHCR) }, () =>
      refusal(503, 'registries_unavailable', 'the registry store is unavailable'),
    )
    const user = userEvent.setup()
    render(<ImageRegistries />)

    const ghcr = await card('ghcr.io')
    await user.click(ghcr.getByRole('button', { name: 'Remove ghcr.io…' }))
    await user.click(ghcr.getByRole('button', { name: 'Remove registry' }))

    const alert = await ghcr.findByRole('alert')
    expect(alert).toHaveTextContent('the registry store is unavailable')
    expect(alert).toHaveTextContent('Try again shortly.')
    expect(screen.getByRole('region', { name: 'ghcr.io' })).toBeInTheDocument()
  })
})
