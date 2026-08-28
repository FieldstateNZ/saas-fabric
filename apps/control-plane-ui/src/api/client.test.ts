import { afterEach, describe, expect, it, vi } from 'vitest'

import { getIdentity, listClients, putIdentity } from './client'
import { ControlPlaneError } from './errors'
import type { Identity } from './types'

/** An identity the API might return. */
const IDENTITY: Identity = {
  realm: 'acme',
  roles: ['Client Realm Administrator', 'Client Realm User'],
  clients: [],
  revision: 'rev-1',
  reconciliation: { status: 'pending', observedAtUnix: null, detail: null },
}

/** Installs a fetch that answers once with the given status and body. */
function answering(status: number, body: unknown): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve(body),
  })

  vi.stubGlobal('fetch', fetchMock)

  return fetchMock
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the control-plane API client', () => {
  it('reads the client list', async () => {
    answering(200, { clients: [{ id: 'acme', displayName: 'Acme' }] })

    const clients = await listClients()

    expect(clients).toHaveLength(1)
    expect(clients[0]?.id).toBe('acme')
  })

  it('calls only the control plane, on the same origin', async () => {
    // The boundary this console exists to hold: every request goes to a
    // relative path on the SaaS Fabric API, so there is no second origin to
    // hold a credential for.
    const fetchMock = answering(200, IDENTITY)

    await getIdentity('acme')

    const [url] = fetchMock.mock.calls[0] as [string]
    expect(url.startsWith('/api/')).toBe(true)
  })

  it('sends the revision it read as the write precondition', async () => {
    const fetchMock = answering(200, IDENTITY)

    await putIdentity('acme', 'rev-1', {
      realm: 'acme',
      roles: ['Client Realm Administrator', 'Client Realm User'],
      clients: [],
    })

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    const headers = new Headers(init.headers)

    expect(headers.get('If-Match')).toBe('"rev-1"')
    expect(init.method).toBe('PUT')
  })

  it('escapes a client id rather than interpolating it into a path', async () => {
    // The property is that the id contributes no path separator, so a value
    // containing one addresses the same resource rather than a different one.
    const fetchMock = answering(200, IDENTITY)

    await getIdentity('acme/../other')

    const [url] = fetchMock.mock.calls[0] as [string]
    expect(url.split('/')).toStrictEqual(['', 'api', 'clients', 'acme%2F..%2Fother', 'identity'])
  })

  it('reports a conflict as a conflict so the caller can re-read', async () => {
    answering(409, {
      error: { code: 'revision_conflict', message: 'the client changed since it was read' },
    })

    const error = await putIdentity('acme', 'rev-1', {
      realm: 'acme',
      roles: [],
      clients: [],
    }).catch((thrown: unknown) => thrown)

    expect(error).toBeInstanceOf(ControlPlaneError)
    expect((error as ControlPlaneError).isConflict).toBe(true)
  })

  it('carries the API message rather than inventing one', async () => {
    answering(400, {
      error: { code: 'realm_immutable', message: "a client's realm cannot be changed" },
    })

    const error = (await getIdentity('acme').catch((thrown: unknown) => thrown)) as ControlPlaneError

    expect(error.code).toBe('realm_immutable')
    expect(error.message).toContain('realm cannot be changed')
  })

  it('still produces an actionable error when the body is not the API error shape', async () => {
    // A proxy's HTML error page, say. The console must not surface a JSON
    // parse failure in place of the thing that actually went wrong.
    const fetchMock = vi.fn().mockResolvedValue({
      ok: false,
      status: 502,
      json: () => Promise.reject(new Error('not JSON')),
    })
    vi.stubGlobal('fetch', fetchMock)

    const error = (await listClients().catch((thrown: unknown) => thrown)) as ControlPlaneError

    expect(error).toBeInstanceOf(ControlPlaneError)
    expect(error.status).toBe(502)
  })
})
