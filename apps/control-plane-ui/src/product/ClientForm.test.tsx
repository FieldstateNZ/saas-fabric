import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Catalogue, ClientProductResponse } from '../api/catalogue-types'
import { ClientForm } from './ClientForm'

/**
 * A real `fetch` stub rather than mocked `api/catalogue` functions: the bugs
 * this file guards against (a double-submitted write, a stale-revision
 * conflict) are about what actually crosses the network and how the real
 * `request()`/`ControlPlaneError` plumbing in `api/client.ts` and
 * `api/errors.ts` turns a response into what `ClientForm` sees, so the test
 * is only honest if that plumbing runs for real.
 */
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

const existing: ClientProductResponse = {
  client: { id: 'acme', displayName: 'Acme', hosts: ['acme.example.com'], realm: 'acme', revision: 'r1' },
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

/** Fills every required field of the create-client step, so Continue actually submits. */
async function fillClientDetails(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByRole('textbox', { name: /Client ID/ }), 'acme')
  await user.type(screen.getByRole('textbox', { name: 'Display name *' }), 'Acme')
  await user.type(screen.getByRole('textbox', { name: 'Legal name *' }), 'Acme Ltd')
  await user.type(screen.getByRole('textbox', { name: 'Region *' }), 'New Zealand')
  await user.type(screen.getByRole('textbox', { name: 'Timezone *' }), 'Pacific/Auckland')
}

let calls: { method: string; url: string }[]

beforeEach(() => {
  calls = []
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('ClientForm: no multi-click and no stray Enter can create or save', () => {
  it('repeated Enter from Client details sends no request', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        calls.push({ method: init?.method ?? 'GET', url: input })
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )

    render(<ClientForm catalogue={catalogue} onSaved={vi.fn()} />)
    const user = userEvent.setup()

    await fillClientDetails(user)
    screen.getByRole('button', { name: 'Continue' }).focus()

    // Details → Applications → Review, then one more: the button Enter was
    // pressed on has been unmounted and replaced by the time this fires, so
    // it lands on nothing.
    await user.keyboard('{Enter}')
    await user.keyboard('{Enter}')
    await user.keyboard('{Enter}')

    expect(calls.filter((call) => call.method === 'POST')).toHaveLength(0)
    expect(document.activeElement).not.toBe(screen.queryByRole('button', { name: 'Create client' }))
  })

  it('Enter on a focused Create client sends exactly one POST', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        calls.push({ method: init?.method ?? 'GET', url: input })
        return Promise.resolve(jsonResponse(201, existing))
      }),
    )

    render(<ClientForm catalogue={catalogue} onSaved={vi.fn()} />)
    const user = userEvent.setup()

    await fillClientDetails(user)
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    screen.getByRole('button', { name: 'Create client' }).focus()
    await user.keyboard('{Enter}')

    await waitFor(() => {
      expect(calls.filter((call) => call.method === 'POST')).toHaveLength(1)
    })
  })

  it('a double-click on Continue at Client details does not skip Applications', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        calls.push({ method: init?.method ?? 'GET', url: input })
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )

    render(<ClientForm catalogue={catalogue} onSaved={vi.fn()} />)
    const user = userEvent.setup()

    await fillClientDetails(user)
    await user.dblClick(screen.getByRole('button', { name: 'Continue' }))

    expect(screen.getByText(/No published applications yet/)).toBeInTheDocument()
    expect(calls).toHaveLength(0)
  })

  it('a double-click on Continue at the Applications step reaches Review and sends no request', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        calls.push({ method: init?.method ?? 'GET', url: input })
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )

    render(<ClientForm catalogue={catalogue} existing={existing} onSaved={vi.fn()} />)
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'Continue' }))
    expect(screen.getByText(/No published applications yet/)).toBeInTheDocument()

    // The exploit: the second click of a double-click lands on the same
    // physical button after the first has already advanced it from
    // "Continue" into "Save client configuration".
    await user.dblClick(screen.getByRole('button', { name: 'Continue' }))

    expect(screen.getByRole('heading', { name: 'Acme' })).toBeInTheDocument()
    expect(calls).toHaveLength(0)
  })

  it('one click on Save client configuration sends exactly one PUT', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        calls.push({ method: init?.method ?? 'GET', url: input })
        return Promise.resolve(jsonResponse(200, existing))
      }),
    )

    render(<ClientForm catalogue={catalogue} existing={existing} onSaved={vi.fn()} />)
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Save client configuration' }))

    await waitFor(() => {
      expect(calls.filter((call) => call.method === 'PUT')).toHaveLength(1)
    })
  })

  it('keyboard activation (Enter) on the final action still saves', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        calls.push({ method: init?.method ?? 'GET', url: input })
        return Promise.resolve(jsonResponse(200, existing))
      }),
    )

    render(<ClientForm catalogue={catalogue} existing={existing} onSaved={vi.fn()} />)
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    screen.getByRole('button', { name: 'Save client configuration' }).focus()
    await user.keyboard('{Enter}')

    await waitFor(() => {
      expect(calls.filter((call) => call.method === 'PUT')).toHaveLength(1)
    })
  })
})

describe('ClientForm: creating with a taken ID returns to where the ID is', () => {
  it('sends the operator back to step 0 with the message on the Client ID field', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'
        calls.push({ method, url })
        if (url === '/api/clients' && method === 'POST') {
          return Promise.resolve(
            jsonResponse(409, { error: { code: 'client_exists', message: 'a client named acme already exists' } }),
          )
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )

    render(<ClientForm catalogue={catalogue} onSaved={vi.fn()} />)
    const user = userEvent.setup()

    await fillClientDetails(user)
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Create client' }))

    expect(await screen.findByText('a client named acme already exists')).toBeInTheDocument()
    // Back on step 0 — the Client ID field is on screen to show the message
    // beside, and Review's summary is gone.
    expect(screen.getByRole('textbox', { name: /Client ID/ })).toBeInTheDocument()
    expect(screen.queryByText('Application assignments')).not.toBeInTheDocument()
  })
})

describe('ClientForm: a conflict on save is not retried', () => {
  it('says the client changed, and its reload re-reads the product for the caller', async () => {
    const fresh: ClientProductResponse = {
      ...existing,
      client: { ...existing.client, displayName: 'Acme Renamed', revision: 'r2' },
    }

    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'
        calls.push({ method, url })
        if (url === '/api/clients/acme/product' && method === 'PUT') {
          return Promise.resolve(
            jsonResponse(409, {
              error: { code: 'revision_conflict', message: 'the client changed since it was read' },
            }),
          )
        }
        if (url === '/api/clients/acme/product' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, fresh))
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )

    const onStale = vi.fn()
    render(<ClientForm catalogue={catalogue} existing={existing} onSaved={vi.fn()} onStale={onStale} />)
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Save client configuration' }))

    expect(
      await screen.findByText('This client changed since this form was opened. Your edits here were not saved.'),
    ).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /Reload latest version — discards your unsaved changes/ }))

    await waitFor(() => {
      expect(onStale).toHaveBeenCalledWith(fresh)
    })
    expect(calls.filter((call) => call.method === 'GET' && call.url === '/api/clients/acme/product')).toHaveLength(1)
  })

  it('does not offer reload for a plain validation failure, and does not re-read', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'
        calls.push({ method, url })
        if (url === '/api/clients/acme/product' && method === 'PUT') {
          return Promise.resolve(
            jsonResponse(400, { error: { code: 'invalid_request', message: 'legal name is required' } }),
          )
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )

    render(<ClientForm catalogue={catalogue} existing={existing} onSaved={vi.fn()} />)
    const user = userEvent.setup()

    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Save client configuration' }))

    expect(await screen.findByText('legal name is required')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Reload latest version/ })).not.toBeInTheDocument()
  })
})
