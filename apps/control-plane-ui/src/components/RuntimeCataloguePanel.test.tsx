import { render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { RuntimeCataloguePanel } from './RuntimeCataloguePanel'

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('RuntimeCataloguePanel: what it shows', () => {
  it('renders every derived resource, with its application and version', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        jsonResponse(200, {
          revision: 'rev-1',
          resources: [
            {
              name: 'customers',
              application: 'workspec',
              version: 3,
              dataSource: 'primary',
              collection: 'customer_records',
              keyField: 'id',
              operations: ['read', 'list', 'create'],
              queryableFields: ['id', 'name'],
            },
          ],
        }),
      ),
    )

    render(<RuntimeCataloguePanel />)

    expect(await screen.findByText('customers')).toBeInTheDocument()
    expect(screen.getByText('workspec')).toBeInTheDocument()
    expect(screen.getByText('3')).toBeInTheDocument()
    expect(screen.getByText('read, list, create')).toBeInTheDocument()
    expect(screen.getByText('id, name')).toBeInTheDocument()
  })

  it('says plainly that no application declares a resource yet', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(200, { revision: 'rev-0', resources: [] })),
    )

    render(<RuntimeCataloguePanel />)

    expect(
      await screen.findByText('No application release declares a resource yet.'),
    ).toBeInTheDocument()
  })

  it('shows the desired_state_invalid message when derivation is refused', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        jsonResponse(500, {
          error: {
            code: 'desired_state_invalid',
            message: 'resource "customers" is declared by both workspec and portal',
          },
        }),
      ),
    )

    render(<RuntimeCataloguePanel />)

    expect(
      await screen.findByText('resource "customers" is declared by both workspec and portal'),
    ).toBeInTheDocument()
  })
})
