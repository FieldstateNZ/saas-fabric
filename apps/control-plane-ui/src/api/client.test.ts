import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { getIdentity, listClients, putIdentity, request } from './client'
import { ControlPlaneError } from './errors'
import type { Identity } from './types'
import { resetGatewayStateForTests } from '../session/gateway'

/** An identity the API might return. */
const IDENTITY: Identity = {
  realm: 'acme',
  roles: ['Client Realm Administrator', 'Client Realm User'],
  clients: [],
  apiVersion: 'fabric.fieldstate.nz/v2',
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

/** A `location` whose `assign` calls can be counted, since jsdom's own
 *  `location.assign` is non-configurable and cannot be spied on directly. A
 *  real `hash`/`pathname`/`search` too: `redirectToGatewaySignIn` reads them
 *  (via `rememberDestination`) before it ever calls `assign`. */
function watchNavigation(): { to: () => string | null; times: () => number } {
  const assign = vi.fn()
  vi.stubGlobal('location', {
    hash: window.location.hash,
    pathname: window.location.pathname,
    search: window.location.search,
    assign,
  })

  return {
    to: () => (assign.mock.calls[0] as [string] | undefined)?.[0] ?? null,
    times: () => assign.mock.calls.length,
  }
}

/** A `401`, as `/api/clients` sends it once a session has ended. */
function sessionEnded(): void {
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue({
      ok: false,
      status: 401,
      json: () => Promise.resolve({ error: { code: 'unauthenticated', message: 'session ended' } }),
    }),
  )
}

beforeEach(() => {
  sessionStorage.clear()
  resetGatewayStateForTests()
})

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

/**
 * How `request()` answers a `401`: the gateway's cookie session has ended,
 * and only a document navigation, not a `fetch`, can reach its sign-in.
 */
describe('a 401 that ends the session', () => {
  it('navigates to the gateway sign-in', async () => {
    const navigation = watchNavigation()

    sessionEnded()
    await expect(request('/api/clients')).rejects.toThrow()

    expect(navigation.to()).toBe('/')
  })

  it('navigates once even when two panels each receive a 401 at the same moment', async () => {
    const navigation = watchNavigation()
    sessionEnded()

    await Promise.all([
      request('/api/clients').catch(() => undefined),
      request('/api/platform').catch(() => undefined),
    ])

    expect(navigation.times()).toBe(1)
  })

  it('carries no credential of its own -- the gateway forwards the bearer', async () => {
    const fetchMock = answering(200, { clients: [] })

    await listClients()

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(new Headers(init.headers).has('Authorization')).toBe(false)
  })
})

/**
 * The other half of `gateway.ts`'s loop guard: a `200` from `/api/operator`
 * cannot prove a session works (a lying proxy can produce it forever), so
 * `request()` is where the real evidence -- an actual request answered by
 * anything but a `401` -- clears the redirect count instead.
 */
describe('the redirect count, once a real request answers', () => {
  it('is cleared by the first response that is not a 401', async () => {
    sessionStorage.setItem('fabric.gateway.redirected', '1')
    answering(200, { clients: [] })

    await listClients()

    expect(sessionStorage.getItem('fabric.gateway.redirected')).toBeNull()
  })

  it('is never cleared by a 401 -- the redirect that 401 starts counts on top of it', async () => {
    // Consuming the count belongs to gatewaySession; a 401 here is one more
    // redirect that nothing has yet proven worked.
    sessionStorage.setItem('fabric.gateway.redirected', '1')
    watchNavigation()
    sessionEnded()

    await expect(request('/api/clients')).rejects.toThrow()

    expect(sessionStorage.getItem('fabric.gateway.redirected')).toBe('2')
  })
})
