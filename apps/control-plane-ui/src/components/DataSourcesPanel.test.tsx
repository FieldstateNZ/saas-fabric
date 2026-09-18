import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { ConnectionSelector, DataSource, DataSourceInput, DataSources } from '../api/data-source-types'
import { DataSourcesPanel } from './DataSourcesPanel'

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

const shared: DataSource = {
  id: 'shared-postgres-nz-01',
  revision: 3,
  connector: 'postgres-nz',
  connection: { kind: 'named', name: 'shared' },
  placement: 'shared',
  residency: { region: 'nz', jurisdiction: 'NZ' },
  pool: { maxConnections: 20, idleTimeoutSeconds: 300, acquireTimeoutSeconds: 5 },
  capabilities: { writable: true, acceptsNewTenants: true },
  discriminator: { column: 'tenant_key' },
  labels: { owner: 'platform' },
}

const dedicated: DataSource = {
  id: 'dedicated-acme-01',
  revision: 1,
  connector: 'postgres-nz',
  connection: { kind: 'secret', reference: 'secret/data/acme-db' },
  placement: 'dedicated',
  residency: { region: 'nz', jurisdiction: null },
  pool: { maxConnections: 10, idleTimeoutSeconds: 120, acquireTimeoutSeconds: 5 },
  capabilities: { writable: true, acceptsNewTenants: false },
  discriminator: null,
  labels: {},
}

// A connection kind outside `named`/`secret` -- what a hand edit to the
// platform repository could put there. The platform refuses this at
// declaration and revalidates every held entry on read, but the console
// does not assume that enforcement is bulletproof: see `data-source-draft`'s
// `isDeclarableConnection`.
const handEdited: DataSource = {
  id: 'legacy-oracle-01',
  revision: 1,
  connector: 'oracle-legacy',
  connection: { kind: 'default' },
  placement: 'dedicated',
  residency: { region: 'nz', jurisdiction: null },
  pool: { maxConnections: 5, idleTimeoutSeconds: 60, acquireTimeoutSeconds: 5 },
  capabilities: { writable: true, acceptsNewTenants: false },
  discriminator: null,
  labels: {},
}

function inputFor(dataSource: DataSource): DataSourceInput {
  return {
    connector: dataSource.connector,
    // Every fixture this is called with is known to be declarable; the
    // assertion names that (allowed in tests) rather than widening
    // `DataSourceInput.connection` to match `DataSource`'s honest type.
    connection: dataSource.connection as ConnectionSelector,
    placement: dataSource.placement,
    residency: dataSource.residency,
    pool: dataSource.pool,
    capabilities: dataSource.capabilities,
    discriminator: dataSource.discriminator,
    labels: dataSource.labels,
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the data sources panel: what a row shows', () => {
  it('renders every declared data source, with the discriminator column only where one exists', async () => {
    const stored: DataSources = {
      environment: 'lucentroot',
      revision: 'rev-1',
      dataSources: [shared, dedicated],
    }
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, stored)))

    render(<DataSourcesPanel />)

    expect(await screen.findByText(shared.id)).toBeInTheDocument()
    expect(screen.getByText(dedicated.id)).toBeInTheDocument()
    expect(screen.getByText('Shared')).toBeInTheDocument()
    expect(screen.getByText('Dedicated')).toBeInTheDocument()
    expect(screen.getByText('tenant_key')).toBeInTheDocument()
    // The dedicated row has no discriminator: a shared-only fact, not a blank.
    expect(screen.getAllByText('—')).toHaveLength(1)
    expect(screen.getByText('Named: shared')).toBeInTheDocument()
    expect(screen.getByText('Secret: secret/data/acme-db')).toBeInTheDocument()
  })

  it('says plainly that nothing is declared yet', async () => {
    const stored: DataSources = { environment: 'lucentroot', revision: 'rev-0', dataSources: [] }
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, stored)))

    render(<DataSourcesPanel />)

    expect(await screen.findByText(/no data sources declared/i)).toBeInTheDocument()
  })

  it('renders nothing for a deployment that manages no platform', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        jsonResponse(404, { error: { code: 'platform_not_managed', message: 'not managed' } }),
      ),
    )

    const { container } = render(<DataSourcesPanel />)

    await waitFor(() => {
      expect(container).toBeEmptyDOMElement()
    })
  })

  it('shows a connection outside named/secret as not declarable, and disables Edit for that row', async () => {
    const stored: DataSources = {
      environment: 'lucentroot',
      revision: 'rev-1',
      dataSources: [handEdited],
    }
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, stored)))

    render(<DataSourcesPanel />)

    expect(await screen.findByText('Not declarable')).toBeInTheDocument()
    const editButton = screen.getByRole('button', { name: 'Edit' })
    expect(editButton).toBeDisabled()
    expect(editButton).toHaveAttribute(
      'title',
      'Correct this connection in the platform repository before editing it here.',
    )
  })
})

describe('the data sources panel: the discriminator field tracks placement', () => {
  it('only asks for a discriminator column while placement is shared', async () => {
    const stored: DataSources = { environment: 'lucentroot', revision: 'rev-0', dataSources: [] }
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, stored)))
    const user = userEvent.setup()

    render(<DataSourcesPanel />)
    await user.click(await screen.findByRole('button', { name: '+ Declare data source' }))

    // A new declaration defaults to `shared`, so the field starts visible.
    expect(screen.getByLabelText(/discriminator column/i)).toBeInTheDocument()

    await user.selectOptions(screen.getByRole('combobox', { name: 'Placement' }), 'dedicated')
    expect(screen.queryByLabelText(/discriminator column/i)).not.toBeInTheDocument()

    await user.selectOptions(screen.getByRole('combobox', { name: 'Placement' }), 'shared')
    expect(screen.getByLabelText(/discriminator column/i)).toBeInTheDocument()
  })
})

describe('the data sources panel: the write precondition', () => {
  it('corrects an existing row with If-Match and the revision it was read at', async () => {
    const stored: DataSources = { environment: 'lucentroot', revision: 'rev-1', dataSources: [shared] }
    const calls: { method: string; url: string; ifMatch: string | null; body: unknown }[] = []

    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'

        if (url === '/api/platform/data-sources' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, stored))
        }
        if (url === `/api/platform/data-sources/${shared.id}` && method === 'PUT') {
          calls.push({
            method,
            url,
            ifMatch: new Headers(init?.headers).get('If-Match'),
            body: init?.body,
          })
          return Promise.resolve(jsonResponse(200, { ...stored, revision: 'rev-2' }))
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )
    const user = userEvent.setup()

    render(<DataSourcesPanel />)
    await user.click(await screen.findByRole('button', { name: 'Edit' }))
    await user.click(screen.getByRole('button', { name: 'Save data source' }))

    await waitFor(() => {
      expect(calls).toHaveLength(1)
    })

    expect(calls[0]?.ifMatch).toBe('"rev-1"')
    expect(JSON.parse(calls[0]?.body as string)).toEqual(inputFor(shared))
  })

  it('declares a first data source with If-Match and the tag GET returned, when this environment declares nothing yet', async () => {
    // The late-bound binding always has a generation tag to compare-and-swap
    // on, even before any file exists, so an empty environment still reads
    // a non-null revision and the first declaration conditions on it --
    // there is no If-None-Match: * case on this route any more.
    const empty: DataSources = { environment: 'lucentroot', revision: 'rev-0', dataSources: [] }
    const calls: { ifMatch: string | null }[] = []

    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'

        if (url === '/api/platform/data-sources' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, empty))
        }
        if (url === `/api/platform/data-sources/${shared.id}` && method === 'PUT') {
          calls.push({ ifMatch: new Headers(init?.headers).get('If-Match') })
          return Promise.resolve(jsonResponse(200, { ...empty, revision: 'rev-1', dataSources: [shared] }))
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )
    const user = userEvent.setup()

    render(<DataSourcesPanel />)
    await user.click(await screen.findByRole('button', { name: '+ Declare data source' }))
    await user.type(screen.getByLabelText(/data source id/i), shared.id)
    await user.type(screen.getByLabelText(/^connector/i), shared.connector)
    await user.type(screen.getByLabelText(/^region/i), shared.residency.region)
    await user.type(screen.getByLabelText(/discriminator column/i), shared.discriminator?.column ?? '')
    await user.type(screen.getByLabelText(/connection name/i), 'shared')
    await user.click(screen.getByRole('button', { name: 'Declare data source' }))

    await waitFor(() => {
      expect(calls).toHaveLength(1)
    })

    expect(calls[0]?.ifMatch).toBe('"rev-0"')
  })
})

describe('the data sources panel: pool field validation', () => {
  it('refuses to submit while a pool field is empty, and shows the error beside it', async () => {
    const stored: DataSources = { environment: 'lucentroot', revision: 'rev-1', dataSources: [shared] }
    const putCalls: string[] = []
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'

        if (url === '/api/platform/data-sources' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, stored))
        }
        if (method === 'PUT') {
          putCalls.push(url)
          return Promise.resolve(jsonResponse(200, stored))
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )
    const user = userEvent.setup()

    render(<DataSourcesPanel />)
    await user.click(await screen.findByRole('button', { name: 'Edit' }))
    await user.clear(screen.getByLabelText(/max connections/i))

    expect(await screen.findByText('Enter a number.')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Save data source' }))

    // `hasInvalidPoolFields` returns before `toInput` or `onSubmit` are
    // reached, so a wrongly sent request would already have gone out
    // synchronously with the click above.
    expect(putCalls).toHaveLength(0)
  })
})

describe('the data sources panel: the form follows the row being edited', () => {
  it('shows the second row when Edit is clicked on it while the first is still open', async () => {
    // The form reads its target once, at mount. Without a key tied to the
    // target, React reuses the instance and an operator who clicks Edit on a
    // second row keeps the first row's values under a new heading -- and
    // saves the first row's edits under the second row's id.
    const stored: DataSources = {
      environment: 'lucentroot',
      revision: 'rev-1',
      dataSources: [shared, dedicated],
    }
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.resolve(jsonResponse(200, stored))),
    )
    const user = userEvent.setup()

    render(<DataSourcesPanel />)
    const edits = await screen.findAllByRole('button', { name: 'Edit' })
    await user.click(edits[0] as HTMLElement)
    expect(screen.getByDisplayValue(shared.id)).toBeInTheDocument()

    await user.click(edits[1] as HTMLElement)
    expect(screen.getByDisplayValue(dedicated.id)).toBeInTheDocument()
    expect(screen.queryByDisplayValue(shared.id)).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: '+ Declare data source' }))
    expect(screen.queryByDisplayValue(dedicated.id)).not.toBeInTheDocument()
  })
})

describe('the data sources panel: the Remove flow', () => {
  it('asks for confirmation before sending the DELETE, and cancel backs out without sending anything', async () => {
    const stored: DataSources = { environment: 'lucentroot', revision: 'rev-1', dataSources: [shared] }
    const calls: { method: string; url: string }[] = []

    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'

        if (url === '/api/platform/data-sources' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, stored))
        }
        calls.push({ method, url })
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )
    const user = userEvent.setup()

    render(<DataSourcesPanel />)
    await user.click(await screen.findByRole('button', { name: 'Remove' }))

    expect(screen.getByText(`Remove ${shared.id}?`)).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Cancel' }))

    expect(screen.queryByText(`Remove ${shared.id}?`)).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Remove' })).toBeInTheDocument()
    expect(calls).toHaveLength(0)
  })

  it('confirming sends DELETE with If-Match at the current revision, and a 200 reloads the list', async () => {
    const stored: DataSources = { environment: 'lucentroot', revision: 'rev-1', dataSources: [shared] }
    const calls: { method: string; url: string; ifMatch: string | null }[] = []

    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'

        if (url === '/api/platform/data-sources' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, stored))
        }
        if (url === `/api/platform/data-sources/${shared.id}` && method === 'DELETE') {
          calls.push({ method, url, ifMatch: new Headers(init?.headers).get('If-Match') })
          return Promise.resolve(
            jsonResponse(200, { environment: 'lucentroot', revision: 'rev-2', dataSources: [] }),
          )
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )
    const user = userEvent.setup()

    render(<DataSourcesPanel />)
    await user.click(await screen.findByRole('button', { name: 'Remove' }))
    await user.click(screen.getByRole('button', { name: 'Confirm' }))

    await waitFor(() => {
      expect(calls).toHaveLength(1)
    })
    expect(calls[0]?.ifMatch).toBe('"rev-1"')
    expect(await screen.findByText(/no data sources declared/i)).toBeInTheDocument()
  })

  it('a 409 data_source_in_use shows the message the server sent, naming the tenants', async () => {
    const stored: DataSources = { environment: 'lucentroot', revision: 'rev-1', dataSources: [shared] }

    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'

        if (url === '/api/platform/data-sources' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, stored))
        }
        if (url === `/api/platform/data-sources/${shared.id}` && method === 'DELETE') {
          return Promise.resolve(
            jsonResponse(409, {
              error: { code: 'data_source_in_use', message: 'placed for acme, initech' },
            }),
          )
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )
    const user = userEvent.setup()

    render(<DataSourcesPanel />)
    await user.click(await screen.findByRole('button', { name: 'Remove' }))
    await user.click(screen.getByRole('button', { name: 'Confirm' }))

    expect(await screen.findByText('placed for acme, initech')).toBeInTheDocument()
    // The row is still there -- refused, not removed -- and the confirm step
    // has closed rather than staying open on a request that already failed.
    expect(screen.getByText(shared.id)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Remove' })).toBeInTheDocument()
    // A 409 in use is not a stale write: there is nothing a reload fixes, so
    // the reload affordance must not appear alongside it.
    expect(screen.queryByText(/reload latest version/i)).not.toBeInTheDocument()
  })

  it('a 409 revision_conflict shows the reload affordance instead of the message alone', async () => {
    const stored: DataSources = { environment: 'lucentroot', revision: 'rev-1', dataSources: [shared] }

    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const url = input
        const method = init?.method ?? 'GET'

        if (url === '/api/platform/data-sources' && method === 'GET') {
          return Promise.resolve(jsonResponse(200, stored))
        }
        if (url === `/api/platform/data-sources/${shared.id}` && method === 'DELETE') {
          return Promise.resolve(
            jsonResponse(409, { error: { code: 'revision_conflict', message: 'stale' } }),
          )
        }
        return Promise.resolve(jsonResponse(404, { error: { code: 'not_found', message: 'unused' } }))
      }),
    )
    const user = userEvent.setup()

    render(<DataSourcesPanel />)
    await user.click(await screen.findByRole('button', { name: 'Remove' }))
    await user.click(screen.getByRole('button', { name: 'Confirm' }))

    // Unlike `data_source_in_use`, a stale revision is exactly what a reload
    // fixes -- the same distinction `ClientDataTab` draws for a refused vs a
    // conflicted `Place`.
    expect(await screen.findByText(/reload latest version/i)).toBeInTheDocument()
  })
})
