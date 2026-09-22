/**
 * `gatewaySession`, `isBehindGateway`, and `redirectToGatewaySignIn` in
 * isolation -- what `useSession.test.ts` and `src/api/client.test.ts` cover
 * is how the rest of the console reacts to what this module decides.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { rememberDestination } from './destination'
import {
  clearRedirectCount,
  gatewaySession,
  isBehindGateway,
  redirectToGatewaySignIn,
  resetGatewayStateForTests,
} from './gateway'

/** A `location` with a controllable hash and a spyable `assign`, the way
 *  jsdom's own `location` cannot be (`assign` is non-configurable there). */
function stubLocation(hash = ''): ReturnType<typeof vi.fn> {
  const assign = vi.fn()
  vi.stubGlobal('location', { hash, pathname: '/', search: '', assign })
  return assign
}

/** A `fetch` answering `/api/operator` once with the given status and body. */
function answeringWith(status: number, body: unknown = {}): void {
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue({ ok: status >= 200 && status < 300, status, json: () => Promise.resolve(body) }),
  )
}

/** A `fetch` answering `/api/operator` once with the given status and a
 *  readable body that names no error code -- `{}`, not a body that fails
 *  to parse at all. `answeringWithUnreadableBody` below is that other case. */
function answering(status: number): void {
  answeringWith(status)
}

/** A `fetch` answering `/api/operator` with a body that cannot be parsed as
 *  JSON at all -- a proxy's own HTML error page landing where the control
 *  plane's JSON was expected, say. */
function answeringWithUnreadableBody(status: number): void {
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue({
      ok: status >= 200 && status < 300,
      status,
      json: () => Promise.reject(new SyntaxError('Unexpected token <')),
    }),
  )
}

/** The shape `ControlPlaneError`'s response body actually has. */
function errorBody(code: string): { error: { code: string; message: string } } {
  return { error: { code, message: 'irrelevant to this test' } }
}

beforeEach(() => {
  sessionStorage.clear()
  window.history.replaceState({}, '', '/')
  resetGatewayStateForTests()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('gatewaySession', () => {
  it('reports signed-in on 200, and remembers it for isBehindGateway', async () => {
    answering(200)

    expect(await gatewaySession()).toBe('signed-in')
    expect(isBehindGateway()).toBe(true)
  })

  it('reports no session for a plain 401', async () => {
    answering(401)

    expect(await gatewaySession()).toBe('no-session')
    expect(isBehindGateway()).toBe(false)
  })

  it('reports no session for a 403 too', async () => {
    answering(403)

    expect(await gatewaySession()).toBe('no-session')
  })

  it('throws on a status that is neither a clean yes nor a clean no', async () => {
    sessionStorage.setItem('fabric.gateway.redirected', '1')
    answering(500)

    await expect(gatewaySession()).rejects.toThrow(/500/)
    // A probe that fails partway through must not leave the count for the
    // *next* probe to misread as its own answer -- it is read and cleared
    // before anything here can throw.
    expect(sessionStorage.getItem('fabric.gateway.redirected')).toBeNull()
  })

  it('reports a refusal purely from the body, with no count at all', async () => {
    // The case slice 1's first cut of this file could not close: a first
    // page load, behind a gateway, whose forwarded token the control plane
    // refuses. There is no count for it -- this tab never redirected
    // anywhere -- so the body has to be the one thing that can say so.
    answeringWith(401, errorBody('operator_refused'))

    expect(await gatewaySession()).toBe('refused')
    expect(isBehindGateway()).toBe(false)
  })

  it('reports no session, not a refusal, for a 401 that just means no bearer at all', async () => {
    answeringWith(401, errorBody('unauthenticated'))

    expect(await gatewaySession()).toBe('no-session')
  })

  it('reports a refusal when the redirect count is still there on a 401', async () => {
    sessionStorage.setItem('fabric.gateway.redirected', '1')
    answering(401)

    expect(await gatewaySession()).toBe('refused')
    expect(sessionStorage.getItem('fabric.gateway.redirected')).toBeNull()
  })

  it('does not report a refusal twice for the same count', async () => {
    sessionStorage.setItem('fabric.gateway.redirected', '1')
    answering(401)
    expect(await gatewaySession()).toBe('refused')

    answering(401)
    expect(await gatewaySession()).toBe('no-session')
  })

  it('trusts the body over a stale count when the body says there is no bearer at all', async () => {
    // The count is a belt for an unreadable body, not a vote -- a control
    // plane that explicitly says "unauthenticated" is saying there was no
    // bearer to refuse, whatever this tab remembers redirecting for.
    sessionStorage.setItem('fabric.gateway.redirected', '1')
    answeringWith(401, errorBody('unauthenticated'))

    expect(await gatewaySession()).toBe('no-session')
  })

  it('falls back to the count when the body cannot be read at all', async () => {
    answeringWithUnreadableBody(401)
    expect(await gatewaySession()).toBe('no-session')

    sessionStorage.setItem('fabric.gateway.redirected', '1')
    answeringWithUnreadableBody(401)
    expect(await gatewaySession()).toBe('refused')
  })

  it('does not clear the count merely because the probe answered 200', async () => {
    // The property this file exists to hold now: a lying proxy can answer
    // 200 forever, so a 200 alone must never look like recovery. Only
    // `clearRedirectCount` -- called by `client.ts` on a real request's
    // success, not by this probe -- clears it. See the tests below.
    sessionStorage.setItem('fabric.gateway.redirected', '1')
    answering(200)

    expect(await gatewaySession()).toBe('signed-in')
    expect(sessionStorage.getItem('fabric.gateway.redirected')).toBe('1')
  })

  it('restores the hash route parked before a redirect once a session is found', async () => {
    // The way `redirectToGatewaySignIn` would have left it: a hash route
    // remembered, then a fresh load with no hash -- the gateway's own round
    // trip landed back on '/'.
    window.history.replaceState({}, '', '/#/clients/acme')
    rememberDestination()
    window.history.replaceState({}, '', '/')
    answering(200)

    expect(await gatewaySession()).toBe('signed-in')
    expect(window.location.hash).toBe('#/clients/acme')
  })

  it('reaches looping only after two full loads that each end in a redirect', async () => {
    // The real path, not seeded storage: each load probes first, the way
    // `establish()` always does, and only redirects afterward, once a panel
    // 401s. `resetGatewayStateForTests()` between them stands in for a fresh
    // page load's in-memory state -- `navigating` resets; the count in
    // `sessionStorage` does not, the same way a real reload would leave it.
    const assign = stubLocation()

    // Load 1: the probe says signed-in, but nothing behind it works.
    answering(200)
    expect(await gatewaySession()).toBe('signed-in')
    redirectToGatewaySignIn()

    // Load 2: the same lie, the same redirect.
    resetGatewayStateForTests()
    answering(200)
    expect(await gatewaySession()).toBe('signed-in')
    redirectToGatewaySignIn()

    // Load 3: two redirects have now gone by with nothing to prove either
    // one actually worked.
    resetGatewayStateForTests()
    answering(200)

    expect(await gatewaySession()).toBe('looping')
    // Detecting the loop must not itself be a third redirect.
    expect(assign).toHaveBeenCalledTimes(2)
  })

  it('recovers once a real request succeeds, the way client.ts would clear it', async () => {
    stubLocation()

    // Load 1: signed in, one redirect -- the same one-off hiccup as above.
    answering(200)
    expect(await gatewaySession()).toBe('signed-in')
    redirectToGatewaySignIn()

    // Load 2: still below the threshold, still "signed in" -- but this time
    // a real request succeeds, so `client.ts` clears the count right then.
    resetGatewayStateForTests()
    answering(200)
    expect(await gatewaySession()).toBe('signed-in')
    clearRedirectCount()

    // A later, unrelated session end must not inherit the earlier hiccup.
    answering(200)
    expect(await gatewaySession()).toBe('signed-in')
    expect(sessionStorage.getItem('fabric.gateway.redirected')).toBeNull()
  })
})

describe('redirectToGatewaySignIn', () => {
  it('remembers the hash route and counts the redirect before navigating', () => {
    const assign = stubLocation('#/clients/acme')

    redirectToGatewaySignIn()

    expect(sessionStorage.getItem('fabric.signin.destination')).toBe('#/clients/acme')
    expect(sessionStorage.getItem('fabric.gateway.redirected')).toBe('1')
    expect(assign).toHaveBeenCalledWith('/')
  })

  it('navigates once when two 401s ask for it in the same tick', () => {
    const assign = stubLocation()

    redirectToGatewaySignIn()
    redirectToGatewaySignIn()

    expect(assign).toHaveBeenCalledTimes(1)
  })
})
