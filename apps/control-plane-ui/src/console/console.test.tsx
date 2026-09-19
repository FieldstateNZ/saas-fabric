import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { converge } from '../api/client'
import type { Client, Identity } from '../api/types'
import { ClientDirectory } from './ClientDirectory'
import { Console } from './Console'
import { Dashboard } from './Dashboard'
import { clientHref, readRoute } from './navigation'
import { Reconciliation } from './Reconciliation'
import type { Inventory } from './useInventory'

// `converge` alone is mocked, not the whole module — the Console-level test
// further down needs the real `request()` plumbing (`listClients`,
// `getIntegration`, and the rest) to reach the `fetch` stub it installs.
vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, converge: vi.fn() }
})

const clients: Client[] = [
  { id: 'acme', displayName: 'Acme', hosts: ['acme.example.test'], realm: 'acme', revision: 'r1' },
  {
    id: 'northstar',
    displayName: 'Northstar',
    hosts: ['north.example.test'],
    realm: 'northstar',
    revision: 'r1',
  },
]

const identity: Identity = {
  realm: 'acme',
  roles: [],
  clients: [],
  apiVersion: 'v2',
  revision: 'r1',
  reconciliation: { status: 'pending', observedAtUnix: null, detail: null },
}

const inventory: Inventory = {
  loading: false,
  refresh: vi.fn(),
  entries: clients.map((client) => ({ client, identity, error: null })),
}

afterEach(() => {
  window.history.replaceState({}, '', '/')
  vi.clearAllMocks()
})

describe('phase-one console', () => {
  it('searches domains and clears an empty result', async () => {
    render(<ClientDirectory clients={clients} inventory={inventory} />)

    await userEvent.type(screen.getByRole('textbox', { name: 'Search clients' }), 'north.example')
    expect(screen.queryByRole('link', { name: 'Open Acme' })).toBeNull()
    expect(screen.getByRole('link', { name: 'Open Northstar' })).toHaveAttribute(
      'href',
      '#/clients/northstar',
    )

    await userEvent.selectOptions(screen.getByLabelText('Filter by status'), 'failed')
    expect(screen.getByRole('heading', { name: 'No matching clients' })).toBeInTheDocument()

    await userEvent.click(screen.getByRole('button', { name: 'Clear filters' }))
    expect(screen.getByRole('link', { name: 'Open Acme' })).toBeInTheDocument()
  })

  it('never renders incomplete identity totals as zero or platform health as healthy', () => {
    const incomplete = {
      ...inventory,
      entries: clients.map((client) => ({ client, identity: null, error: 'Access denied' })),
    }
    render(<Dashboard clients={clients} platform={null} inventory={incomplete} />)

    expect(
      within(screen.getByRole('link', { name: /Pending identities/ })).getByText('—'),
    ).toBeInTheDocument()
    expect(screen.getByText('Attention required')).toBeInTheDocument()
    expect(screen.queryByText('Healthy')).toBeNull()
  })

  it('refreshes observations only after a successful convergence request', async () => {
    vi.mocked(converge).mockResolvedValue({ clients: 2 })
    render(<Reconciliation inventory={inventory} connected />)

    await userEvent.click(screen.getByRole('button', { name: 'Check for drift' }))

    expect(await screen.findByText('Checked 2 clients.')).toBeInTheDocument()
    expect(inventory.refresh).toHaveBeenCalledOnce()
  })

  it('retains deep links and falls back safely for malformed hashes', () => {
    window.history.replaceState({}, '', clientHref('a b'))
    expect(readRoute()).toEqual({ page: 'clients', clientId: 'a b' })

    window.history.replaceState({}, '', '#/clients/%E0%A4%A')
    expect(readRoute()).toEqual({ page: 'clients', clientId: null })

    window.history.replaceState({}, '', '#/does-not-exist')
    expect(readRoute().page).toBe('overview')
  })

  it('lands integration callbacks on Integrations', () => {
    window.history.replaceState({}, '', '/?platform=connected#/clients')
    expect(readRoute().page).toBe('integrations')
  })
})

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const catalogueBody = () => ({
  catalogue: {
    applications: [],
    clientFields: [],
    settings: { platformName: 'Fieldstate', defaultRegion: 'New Zealand', timezone: 'Pacific/Auckland' },
    environments: [],
    activity: [],
    definitionVersion: 1,
  },
  revision: 'rev-1',
})

const emptyApplicationDefinition = (name: string) => ({
  name,
  description: '',
  domain: '',
  components: [],
  features: [],
  plans: [],
  fields: [],
  navigation: [],
  resources: [],
})

const catalogueBodyWithApplications = () => ({
  catalogue: {
    ...catalogueBody().catalogue,
    applications: [
      { id: 'app-a', draft: emptyApplicationDefinition('App A'), releases: [] },
      { id: 'app-b', draft: emptyApplicationDefinition('App B'), releases: [] },
    ],
  },
  revision: 'rev-1',
})

/** The three fixed stubs almost every `<Console />` test needs regardless of what it is testing. */
function platformStub(url: string): Response | undefined {
  if (url === '/api/integrations/git') {
    return jsonResponse(200, {
      status: 'not_configured',
      connection: null,
      last_success_at: null,
      managed: true,
      application: null,
    })
  }
  if (url === '/api/integrations/platform') {
    return jsonResponse(200, { managed: false, application: null })
  }
  if (url === '/api/platform') {
    return jsonResponse(404, { error: { code: 'platform_not_managed', message: 'not managed' } })
  }
  return undefined
}

describe('Console: a save refusal does not follow an operator to another page', () => {
  let saveStatus = 200

  beforeEach(() => {
    saveStatus = 200
    window.history.replaceState({}, '', '/#/settings')
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('clears the reload prompt on Settings once the operator navigates to Definition', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'
        if (url === '/api/clients' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, { clients: [] }))
        }
        if (url === '/api/catalogue' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, catalogueBody()))
        }
        if (url === '/api/catalogue' && method === 'POST') {
          return Promise.resolve(
            saveStatus === 200
              ? jsonResponse(200, catalogueBody())
              : jsonResponse(409, { error: { code: 'revision_conflict', message: 'stale' } }),
          )
        }
        if (url === '/api/integrations/git') {
          return Promise.resolve(
            jsonResponse(200, {
              status: 'not_configured',
              connection: null,
              last_success_at: null,
              managed: true,
              application: null,
            }),
          )
        }
        if (url === '/api/integrations/platform') {
          return Promise.resolve(jsonResponse(200, { managed: false, application: null }))
        }
        if (url === '/api/platform') {
          return Promise.resolve(
            jsonResponse(404, { error: { code: 'platform_not_managed', message: 'not managed' } }),
          )
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )

    render(<Console />)
    const user = userEvent.setup()

    await screen.findByRole('heading', { name: 'Settings' })

    saveStatus = 409
    await user.click(screen.getByRole('button', { name: 'Save settings' }))
    await screen.findByRole('button', { name: /Reload latest version/ })

    window.location.hash = '/definition'

    await screen.findByRole('heading', { name: 'Client definition' })
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: /Reload latest version/ })).not.toBeInTheDocument()
    })
  })
})

describe('Console: identity is not re-read on pages that do not show it', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('reads identity once on mount, and not again navigating Settings, Definition and Environments', async () => {
    let identityReads = 0

    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'
        if (url === '/api/clients' && method === 'GET') {
          return Promise.resolve(
            jsonResponse(200, {
              clients: [{ id: 'acme', displayName: 'Acme', hosts: [], realm: 'acme', revision: 'r1' }],
            }),
          )
        }
        if (url === '/api/clients/acme/identity') {
          identityReads += 1
          return Promise.resolve(
            jsonResponse(200, {
              realm: 'acme',
              roles: [],
              clients: [],
              apiVersion: 'v2',
              revision: 'r1',
              reconciliation: { status: 'applied', observedAtUnix: null, detail: null },
            }),
          )
        }
        if (url === '/api/catalogue' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, catalogueBody()))
        }
        if (url === '/api/integrations/git') {
          return Promise.resolve(
            jsonResponse(200, {
              status: 'not_configured',
              connection: null,
              last_success_at: null,
              managed: true,
              application: null,
            }),
          )
        }
        if (url === '/api/integrations/platform') {
          return Promise.resolve(jsonResponse(200, { managed: false, application: null }))
        }
        if (url === '/api/platform') {
          return Promise.resolve(
            jsonResponse(404, { error: { code: 'platform_not_managed', message: 'not managed' } }),
          )
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )

    window.history.replaceState({}, '', '/#/overview')
    render(<Console />)

    await screen.findByRole('heading', { name: 'Fieldstate' })
    await waitFor(() => {
      expect(identityReads).toBe(1)
    })

    for (const hash of ['/settings', '/definition', '/environments']) {
      window.location.hash = hash
      await waitFor(() => {
        expect(screen.getByRole('main')).toBeInTheDocument()
      })
    }

    expect(identityReads).toBe(1)
  })
})

describe('Console: a save refusal does not follow an operator to another application', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('offers no Reload for application B once a 409 on application A put it there', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'
        if (url === '/api/clients' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, { clients: [] }))
        }
        if (url === '/api/catalogue' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, catalogueBodyWithApplications()))
        }
        if (url === '/api/catalogue' && method === 'POST') {
          return Promise.resolve(
            jsonResponse(409, {
              error: { code: 'revision_conflict', message: 'the catalogue changed since it was read' },
            }),
          )
        }
        const stub = platformStub(url)
        if (stub) {
          return Promise.resolve(stub)
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )

    window.history.replaceState({}, '', '/#/applications/app-a')
    render(<Console />)
    const user = userEvent.setup()

    await screen.findByRole('heading', { name: 'App A' })
    await user.type(screen.getByRole('textbox', { name: 'Name *' }), ' Renamed')
    await user.click(screen.getByRole('button', { name: 'Save draft' }))
    await screen.findByRole('button', { name: /Reload latest version/ })

    // `applications` is one route shared by every application, told apart
    // only by `applicationId` — B's `ApplicationWorkspace` reads the same
    // `saveError` A's did, unless `Console` clears it on this change too.
    window.location.hash = '/applications/app-b'

    await screen.findByRole('heading', { name: 'App B' })
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: /Reload latest version/ })).not.toBeInTheDocument()
    })
  })
})

describe('Console: a save that lands after the operator has moved on names the page it happened on', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('shows a one-line notice for Settings, with no Reload, once Definition is current', async () => {
    let resolveSave: (response: Response) => void = () => {
      throw new Error('resolveSave called before the delayed POST was issued')
    }

    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'
        if (url === '/api/clients' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, { clients: [] }))
        }
        if (url === '/api/catalogue' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, catalogueBody()))
        }
        if (url === '/api/catalogue' && method === 'POST') {
          return new Promise<Response>((resolve) => {
            resolveSave = resolve
          })
        }
        const stub = platformStub(url)
        if (stub) {
          return Promise.resolve(stub)
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )

    window.history.replaceState({}, '', '/#/settings')
    render(<Console />)
    const user = userEvent.setup()

    await screen.findByRole('heading', { name: 'Settings' })
    await user.click(screen.getByRole('button', { name: 'Save settings' }))

    // The operator moves to Definition before that save's response lands.
    window.location.hash = '/definition'
    await screen.findByRole('heading', { name: 'Client definition' })

    await act(async () => {
      resolveSave(
        jsonResponse(409, {
          error: { code: 'revision_conflict', message: 'the catalogue changed since it was read' },
        }),
      )
      await Promise.resolve()
    })

    expect(
      await screen.findByText('Your change to Settings was not saved: the catalogue changed since it was read'),
    ).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Reload latest version/ })).not.toBeInTheDocument()
  })
})
