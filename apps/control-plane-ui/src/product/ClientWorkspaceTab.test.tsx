import { render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { Catalogue, ClientProductResponse } from '../api/catalogue-types'
import type { Placements } from '../api/placement-types'
import { ClientWorkspaceTab } from './ClientWorkspaceTab'

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const catalogue: Catalogue = {
  applications: [],
  clientFields: [],
  settings: { platformName: 'Fieldstate', defaultRegion: 'New Zealand', timezone: 'Pacific/Auckland' },
  environments: [],
  activity: [],
  definitionVersion: 1,
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

afterEach(() => {
  vi.unstubAllGlobals()
})

describe("ClientWorkspaceTab: tab: 'Data'", () => {
  it('renders ClientDataTab, not some other section', async () => {
    const empty: Placements = {
      clientId: 'acme',
      environment: 'lucentroot',
      revision: 'rev-0',
      placements: [],
    }
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, empty)))

    render(
      <ClientWorkspaceTab tab="Data" data={product()} catalogue={catalogue} onPreview={vi.fn()} />,
    )

    // `ClientDataTab` is the only section that renders a "Data" panel and
    // this empty-placements copy -- proof the switch's `case 'Data'` is what
    // rendered, not a coincidence of some other tab's markup.
    expect(await screen.findByRole('heading', { name: 'Data' })).toBeInTheDocument()
    expect(screen.getByText(/declares no data sources/i)).toBeInTheDocument()
  })
})
