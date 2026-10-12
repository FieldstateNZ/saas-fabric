import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { ClientProductResponse } from '../api/catalogue-types'
import { SAVED_INTENT } from '../api/lifecycle.fixture'
import type { PlacementEntry, Placements, UnboundDataSource } from '../api/placement-types'
import { ClientDataTab } from './ClientDataTab'
import { UnboundResources } from './UnboundResources'

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

function product(): ClientProductResponse {
  return {
    client: { id: 'acme', displayName: 'Acme', hosts: [], realm: 'acme', revision: 'r1', lifecycle: SAVED_INTENT, projection: { state: 'consistent' } },
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
    reconciliation: { status: 'applied', observedAtUnix: null, detail: null, handEdits: [], withheld: [] },
  }
}

const primary: PlacementEntry = {
  logical: 'primary',
  intent: { class: 'shared', provider: null, region: 'nz' },
  placed: null,
  refusal: null,
}

const unboundPrimary: UnboundDataSource = {
  logical: 'primary',
  declared: true,
  resources: [{ resource: 'customers', application: 'workspec', version: 2 }],
}

const unboundReference: UnboundDataSource = {
  logical: 'reference',
  declared: false,
  resources: [
    { resource: 'rates', application: 'ledger', version: 1 },
    { resource: 'currencies', application: 'ledger', version: 1 },
  ],
}

const stored: Placements = {
  clientId: 'acme',
  environment: 'lucentroot',
  revision: 'rev-1',
  placements: [primary],
  unbound: { state: 'checked', dataSources: [unboundPrimary, unboundReference] },
}

function panel(): HTMLElement {
  const section = screen.getByRole('heading', { name: 'Unreachable resources' }).closest('section')
  if (section === null) {
    throw new Error('no Unreachable resources panel')
  }
  return section
}

function unboundRow(logical: string): HTMLElement {
  const row = within(panel()).getByText(logical).closest('tr')
  if (row === null) {
    throw new Error(`no unbound row for ${logical}`)
  }
  return row
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('Unreachable resources: what the panel shows', () => {
  it('lists each unbound logical data source with every resource on it and its release', () => {
    render(<UnboundResources unbound={stored.unbound} />)

    expect(within(panel()).getByText(/UnboundDataSource/)).toBeInTheDocument()
    expect(within(unboundRow('primary')).getByText('customers')).toBeInTheDocument()
    expect(within(unboundRow('primary')).getByText(/workspec v2/)).toBeInTheDocument()
    expect(within(unboundRow('reference')).getByText('rates')).toBeInTheDocument()
    expect(within(unboundRow('reference')).getByText('currencies')).toBeInTheDocument()
    expect(within(unboundRow('reference')).getAllByText(/ledger v1/)).toHaveLength(2)
  })

  it('tells a declared logical data source to be placed, and an undeclared one to be added first', () => {
    render(<UnboundResources unbound={stored.unbound} />)

    expect(within(unboundRow('primary')).getByText('Place it above.')).toBeInTheDocument()
    expect(
      within(unboundRow('reference')).getByText(/add it to this client.s data, then place it/i),
    ).toBeInTheDocument()
  })

  it('says every resource is reachable when nothing is unbound', () => {
    render(<UnboundResources unbound={{ state: 'checked', dataSources: [] }} />)

    expect(within(panel()).getByText(/every resource in the runtime catalogue/i)).toBeInTheDocument()
    expect(within(panel()).queryByRole('table')).not.toBeInTheDocument()
  })

  it('says why nothing was checked when the catalogue could not be read', () => {
    render(
      <UnboundResources
        unbound={{ state: 'unchecked', reason: "resource 'customers' is declared by both 'a' and 'b'" }}
      />,
    )

    expect(within(panel()).getByRole('alert')).toHaveTextContent(
      "Could not check this client against the runtime catalogue: resource 'customers' is declared by both 'a' and 'b'",
    )
    expect(within(panel()).queryByRole('table')).not.toBeInTheDocument()
  })
})

describe('Unreachable resources: on the Data tab', () => {
  it('drops a logical data source from the panel once Place binds it, from the write response', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const method = init?.method ?? 'GET'
        if (input === '/api/clients/acme/placements' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, stored))
        }
        if (input === '/api/clients/acme/placements/primary' && method === 'POST') {
          return Promise.resolve(
            jsonResponse(200, {
              ...stored,
              revision: 'rev-2',
              placements: [
                {
                  ...primary,
                  placed: {
                    dataSource: 'shared-postgres-nz-01',
                    isolation: { kind: 'discriminator', column: 'tenant_key', value: 'acme' },
                    placedAt: '2026-10-12T02:14:00Z',
                  },
                },
              ],
              unbound: { state: 'checked', dataSources: [unboundReference] },
            }),
          )
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )
    const user = userEvent.setup()

    render(<ClientDataTab data={product()} />)
    await screen.findByRole('heading', { name: 'Unreachable resources' })
    expect(within(panel()).getByText('primary')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Place' }))

    expect(await screen.findByText('shared-postgres-nz-01')).toBeInTheDocument()
    expect(within(panel()).queryByText('primary')).not.toBeInTheDocument()
    expect(within(panel()).getByText('reference')).toBeInTheDocument()
  })

  it('still shows the placements when the catalogue could not be checked', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        jsonResponse(200, { ...stored, unbound: { state: 'unchecked', reason: 'unavailable' } }),
      ),
    )

    render(<ClientDataTab data={product()} />)

    expect(await screen.findByRole('button', { name: 'Place' })).toBeEnabled()
    expect(within(panel()).getByRole('alert')).toHaveTextContent('unavailable')
  })
})
