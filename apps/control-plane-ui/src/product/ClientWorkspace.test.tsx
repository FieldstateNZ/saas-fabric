import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Catalogue, ClientProductResponse } from '../api/catalogue-types'
import { ClientWorkspace } from './ClientWorkspace'

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

function product(revision: string): ClientProductResponse {
  return {
    client: { id: 'acme', displayName: 'Acme', hosts: [], realm: 'acme', revision },
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

let calls: { method: string; url: string; ifMatch: string | null }[]

beforeEach(() => {
  calls = []
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('ClientWorkspace: Configure only opens after a fresh read', () => {
  it('re-reads the product before opening the form, so a revision moved elsewhere is not overwritten', async () => {
    // The initial page load reads r1. Something outside this component — an
    // identity edit is the real-world case — has since moved the client to
    // r2 on the server. Configure must pick that up rather than reuse r1.
    let reads = 0

    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'
        const ifMatch = new Headers(init?.headers).get('If-Match')
        calls.push({ method, url, ifMatch })

        if (url === '/api/clients/acme/product' && method === 'GET') {
          reads += 1
          return Promise.resolve(jsonResponse(200, product(reads === 1 ? 'r1' : 'r2')))
        }
        if (url === '/api/clients/acme/product' && method === 'PUT') {
          return Promise.resolve(jsonResponse(200, product('r3')))
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )

    render(<ClientWorkspace id="acme" catalogue={catalogue} onSaved={vi.fn()} />)
    await screen.findByRole('heading', { name: 'Acme' })
    expect(reads).toBe(1)

    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Configure client' }))
    await screen.findByRole('heading', { name: 'Configure Acme' })
    expect(reads).toBe(2)

    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Save client configuration' }))

    await waitFor(() => {
      const put = calls.find((call) => call.method === 'PUT')
      expect(put?.ifMatch).toBe('"r2"')
    })
  })
})
