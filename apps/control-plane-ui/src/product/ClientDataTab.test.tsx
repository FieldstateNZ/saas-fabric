import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { ClientProductResponse } from '../api/catalogue-types'
import type { PlacementEntry, Placements } from '../api/placement-types'
import { ClientDataTab } from './ClientDataTab'

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

function product(): ClientProductResponse {
  return {
    client: { id: 'acme', displayName: 'Acme', hosts: [], realm: 'acme', revision: 'r1' },
    product: {
      legalName: 'Acme Ltd',
      region: 'New Zealand',
      timezone: 'Pacific/Auckland',
      definitionVersion: 1,
      configuration: {},
      applications: [],
      activity: [],
    },
    resolved: [],
    reconciliation: { status: 'applied', observedAtUnix: null, detail: null },
  }
}

const placeable: PlacementEntry = {
  logical: 'primary',
  intent: { class: 'shared', provider: 'postgres', region: 'nz' },
  placed: null,
  refusal: null,
}

const placed: PlacementEntry = {
  logical: 'secondary',
  intent: { class: 'dedicated', provider: null, region: null },
  placed: {
    dataSource: 'dedicated-x-01',
    isolation: { kind: 'database' },
    placedAt: '2026-09-18T02:14:00Z',
  },
  refusal: null,
}

const refused: PlacementEntry = {
  logical: 'audit',
  intent: { class: 'dedicated', provider: null, region: null },
  placed: null,
  refusal: 'no dedicated data source accepts a new tenant',
}

const stored: Placements = {
  clientId: 'acme',
  environment: 'lucentroot',
  revision: 'rev-1',
  placements: [placeable, placed, refused],
}

function rowFor(logical: string): HTMLElement {
  const cell = screen.getByText(logical)
  const row = cell.closest('tr')
  if (row === null) {
    throw new Error(`no row for ${logical}`)
  }
  return row
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the Data tab: what a row shows', () => {
  it('renders the intent, the placement, and the refusal, one row per logical', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, stored)))

    render(<ClientDataTab data={product()} />)

    expect(await screen.findByText('primary')).toBeInTheDocument()
    expect(within(rowFor('primary')).getByText(/shared/i)).toBeInTheDocument()
    expect(within(rowFor('primary')).getByText(/postgres/)).toBeInTheDocument()
    expect(within(rowFor('secondary')).getByText('dedicated-x-01')).toBeInTheDocument()
    expect(within(rowFor('secondary')).getByText(/database/i)).toBeInTheDocument()
    expect(within(rowFor('audit')).getByText(refused.refusal ?? '')).toBeInTheDocument()
  })

  it('explains that placing data needs Platform Management connected, for a deployment that manages no platform', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        jsonResponse(404, { error: { code: 'platform_not_managed', message: 'not managed' } }),
      ),
    )

    render(<ClientDataTab data={product()} />)

    expect(await screen.findByText('Connect platform management')).toBeInTheDocument()
    expect(
      screen.getByText(/placing this client.s data needs platform management connected/i),
    ).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /manage integrations/i })).toHaveAttribute(
      'href',
      '#/integrations',
    )
  })

  it('renders nothing else for a deployment that manages no platform', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        jsonResponse(404, { error: { code: 'platform_not_managed', message: 'not managed' } }),
      ),
    )

    const { container } = render(<ClientDataTab data={product()} />)
    await screen.findByText('Connect platform management')

    expect(container.querySelectorAll('section')).toHaveLength(1)
    expect(screen.queryByRole('table')).not.toBeInTheDocument()
    expect(screen.queryByText(/loading data placement/i)).not.toBeInTheDocument()
  })
})

describe('the Data tab: Place is enabled only when placeable', () => {
  it('enables Place on the row with no placement and no refusal, and disables it on the others', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, stored)))

    render(<ClientDataTab data={product()} />)
    await screen.findByText('primary')

    expect(within(rowFor('primary')).getByRole('button', { name: 'Place' })).toBeEnabled()
    expect(within(rowFor('secondary')).getByRole('button', { name: 'Place' })).toBeDisabled()
    expect(within(rowFor('audit')).getByRole('button', { name: 'Place' })).toBeDisabled()
  })

  it('sends If-Match with the placements revision when Place is clicked', async () => {
    const calls: { method: string; url: string; ifMatch: string | null }[] = []

    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'

        if (url === '/api/clients/acme/placements' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, stored))
        }
        if (url === '/api/clients/acme/placements/primary' && method === 'POST') {
          calls.push({ method, url, ifMatch: new Headers(init?.headers).get('If-Match') })
          return Promise.resolve(
            jsonResponse(200, {
              ...stored,
              revision: 'rev-2',
              placements: [
                { ...placeable, placed: placed.placed, refusal: null },
                placed,
                refused,
              ],
            }),
          )
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )
    const user = userEvent.setup()

    render(<ClientDataTab data={product()} />)
    await screen.findByText('primary')
    await user.click(within(rowFor('primary')).getByRole('button', { name: 'Place' }))

    await waitFor(() => {
      expect(calls).toHaveLength(1)
    })
    expect(calls[0]?.ifMatch).toBe('"rev-1"')
  })
})

describe('the Data tab: how a refusal is shown', () => {
  it('shows a 422 placement_refused message beside the row it is about', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'

        if (url === '/api/clients/acme/placements' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, stored))
        }
        if (url === '/api/clients/acme/placements/primary' && method === 'POST') {
          return Promise.resolve(
            jsonResponse(422, {
              error: { code: 'placement_refused', message: 'no shared data source accepts a new tenant' },
            }),
          )
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )
    const user = userEvent.setup()

    render(<ClientDataTab data={product()} />)
    await screen.findByText('primary')
    await user.click(within(rowFor('primary')).getByRole('button', { name: 'Place' }))

    expect(
      await within(rowFor('primary')).findByText('no shared data source accepts a new tenant'),
    ).toBeInTheDocument()
    expect(screen.queryByText(/reload latest version/i)).not.toBeInTheDocument()
  })

  it('shows a 409 revision_conflict as the reload affordance, not beside the row', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'

        if (url === '/api/clients/acme/placements' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, stored))
        }
        if (url === '/api/clients/acme/placements/primary' && method === 'POST') {
          return Promise.resolve(
            jsonResponse(409, { error: { code: 'revision_conflict', message: 'stale' } }),
          )
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )
    const user = userEvent.setup()

    render(<ClientDataTab data={product()} />)
    await screen.findByText('primary')
    await user.click(within(rowFor('primary')).getByRole('button', { name: 'Place' }))

    expect(await screen.findByText(/reload latest version/i)).toBeInTheDocument()
    expect(within(rowFor('primary')).queryByText('stale')).not.toBeInTheDocument()
  })
})
