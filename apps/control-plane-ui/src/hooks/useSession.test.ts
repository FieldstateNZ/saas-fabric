/**
 * What a signed-out page load does before it gives up and asks.
 *
 * The token is held in memory, so every refresh, every new tab and both round
 * trips through GitHub arrive here with nothing in hand. What matters is that
 * the console asks the provider before it asks the operator — and that it does
 * so at most once, because a console that bounces off its identity provider
 * forever is worse than one that shows a button.
 */
import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { forgetToken } from '../session/session'
import { useSession } from './useSession'
import { request } from '../api/client'
import { currentToken } from '../session/session'
import { redirectToGatewaySignIn, resetGatewayStateForTests } from '../session/gateway'

/** Puts the page at a URL, without navigating. */
function arriveAt(query: string): void {
  window.history.replaceState({}, '', `/${query}`)
}

/** Captures where the console tried to send the browser. */
function watchNavigation(): { to: () => string | null } {
  let destination: string | null = null

  // Listed field by field rather than spread from the real `location`: that is
  // a class instance, so a copy of it is a copy without its prototype. jsdom
  // also makes `assign` non-configurable, which rules out spying on it.
  vi.stubGlobal('location', {
    search: window.location.search,
    pathname: window.location.pathname,
    hash: window.location.hash,
    assign: (url: string) => {
      destination = url
    },
  })

  return { to: () => destination }
}

/**
 * A fetch that answers `/api/operator` with `401` -- no gateway session --
 * and everything else with `otherwise`.
 *
 * Every scenario below except "the gateway probe" describe block is the
 * standalone case: no gateway in front, so the probe always comes back
 * empty-handed and the console's own flow is what is under test.
 */
function noGatewaySession(otherwise: () => { ok: boolean; status?: number; json: () => Promise<unknown> }): ReturnType<typeof vi.fn> {
  // `client.ts` and `gateway.ts` only ever call `fetch` with a string path,
  // never a `Request` or `URL` -- so the mock only has to understand that.
  const fetchMock = vi.fn((url: string) => {
    if (url.startsWith('/api/operator')) {
      return Promise.resolve({
        ok: false,
        status: 401,
        json: () => Promise.resolve({ error: { code: 'unauthenticated', message: 'request carries no operator identity' } }),
      })
    }
    return Promise.resolve(otherwise())
  })
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

/** `/api/session` answering with somewhere to sign in, behind an absent gateway session. */
function providerConfigured(): void {
  noGatewaySession(() => ({
    ok: true,
    json: () =>
      Promise.resolve({
        authorization_endpoint: 'https://identity.example/auth',
        client_id: 'saas-fabric-console',
        redirect_uri: 'https://console.example/',
        scope: 'openid',
      }),
  }))
}

beforeEach(() => {
  sessionStorage.clear()
  forgetToken()
  resetGatewayStateForTests()
  arriveAt('')
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('a signed-out page load', () => {
  it('asks the provider silently before asking the operator', async () => {
    providerConfigured()
    const navigation = watchNavigation()

    renderHook(() => useSession())

    await waitFor(() => {
      expect(navigation.to()).not.toBeNull()
    })

    const url = new URL(navigation.to() as string)
    expect(url.searchParams.get('prompt')).toBe('none')
    expect(sessionStorage.getItem('fabric.signin.silent')).not.toBeNull()
  })

  it('shows the button without alarm when the provider has no session', async () => {
    sessionStorage.setItem('fabric.signin.silent', 'yes')
    arriveAt('?error=login_required&state=whatever')
    providerConfigured()

    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state).toEqual({ status: 'signed-out', error: null, canRetry: true })
    })
    // Accounted for, so the next load is free to try again.
    expect(sessionStorage.getItem('fabric.signin.silent')).toBeNull()
    expect(window.location.search).toBe('')
  })

  it('surfaces an error that is not simply "no session"', async () => {
    sessionStorage.setItem('fabric.signin.silent', 'yes')
    arriveAt('?error=invalid_client&state=whatever')
    providerConfigured()

    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state).toMatchObject({ status: 'signed-out' })
    })
    expect((result.current.state as { error: string | null }).error).toMatch(/invalid_client/)
  })

  it('does not try twice when a callback comes back carrying nothing', async () => {
    sessionStorage.setItem('fabric.signin.silent', 'yes')
    providerConfigured()
    const navigation = watchNavigation()

    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state).toEqual({ status: 'signed-out', error: null, canRetry: true })
    })
    expect(navigation.to()).toBeNull()
    expect(sessionStorage.getItem('fabric.signin.silent')).toBeNull()
  })
})


describe('a session rejected during use', () => {
  async function signedIn() {
    sessionStorage.setItem('fabric.signin.state', 'state')
    sessionStorage.setItem('fabric.signin.verifier', 'verifier')
    arriveAt('?code=code&state=state')
    noGatewaySession(() => ({ ok: true, json: () => Promise.resolve({ access_token: 'current-token' }) }))
    const hook = renderHook(useSession)
    await waitFor(() => { expect(hook.result.current.state.status).toBe('signed-in') })
    return hook
  }

  it('returns to sign-in on 401 without retrying an operator write', async () => {
    const { result } = await signedIn()
    const fetched = vi.fn().mockResolvedValue({ ok: false, status: 401,
      json: () => Promise.resolve({ error: { code: 'unauthenticated', message: 'not a platform operator' } }) })
    vi.stubGlobal('fetch', fetched)
    await act(async () => {
      await expect(request('/api/reconciliation', { method: 'POST' })).rejects.toThrow()
    })
    expect(result.current.state).toEqual({
      status: 'signed-out',
      error: 'Your session ended. Sign in again to continue.',
      canRetry: true,
    })
    expect(currentToken()).toBeNull()
    expect(fetched).toHaveBeenCalledTimes(1)
  })

  it('keeps the session for a permission refusal rather than starting a sign-in loop', async () => {
    const { result } = await signedIn()
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 403,
      json: () => Promise.resolve({ error: { code: 'forbidden', message: 'Operation not permitted' } }) }))
    await act(async () => { await expect(request('/api/platform')).rejects.toThrow() })
    expect(result.current.state.status).toBe('signed-in')
    expect(currentToken()).toBe('current-token')
  })

  it('does not forget a current token for a delayed rejection of an older token', async () => {
    const { result } = await signedIn()
    act(() => { forgetToken('old-token') })
    expect(result.current.state.status).toBe('signed-in')
    expect(currentToken()).toBe('current-token')
  })
})

describe('the gateway probe', () => {
  it('treats a control-plane failure as a real failure, not "no gateway"', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
      ok: false,
      status: 500,
      json: () => Promise.resolve({}),
    }))

    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state.status).toBe('signed-out')
    })
    expect((result.current.state as { error: string | null }).error).toMatch(/500/)
  })

  it('signs in from the gateway session alone, asking nothing else', async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, status: 200, json: () => Promise.resolve({}) })
    vi.stubGlobal('fetch', fetchMock)

    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state).toEqual({ status: 'signed-in' })
    })

    // Exactly one call, to exactly this route: the callback-error and
    // redeem-a-code checks ahead of the probe both ran and found nothing to
    // do, without touching `fetch` themselves.
    expect(fetchMock.mock.calls).toEqual([['/api/operator', expect.anything()]])
  })

  it('reports a refusal, without trying a silent sign-in, when the gateway redirected here and the token was refused', async () => {
    sessionStorage.setItem('fabric.gateway.redirected', '1')
    // Leftover from an unrelated, abandoned attempt -- the refusal's cleanup
    // must clear this the same way a callback error's does, not just leave
    // it for whatever runs next to trip over.
    sessionStorage.setItem('fabric.signin.silent', 'yes')
    const fetchMock = vi.fn().mockResolvedValue({ ok: false, status: 401, json: () => Promise.resolve({}) })
    vi.stubGlobal('fetch', fetchMock)

    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state.status).toBe('signed-out')
    })
    expect(result.current.state).toMatchObject({ canRetry: false })
    expect((result.current.state as { error: string | null }).error).toMatch(/gateway signed you in/)
    expect(sessionStorage.getItem('fabric.signin.silent')).toBeNull()

    const requested = (fetchMock.mock.calls as [string][]).map(([url]) => url)
    expect(requested).not.toContain('/api/session')
  })

  it('reports the loop rather than signing in again, once two full loads have each redirected', async () => {
    // The real path, not seeded storage: each load's probe runs first, the
    // way `establish()` always does, and `redirectToGatewaySignIn` -- what
    // `client.ts` would call once a panel 401s -- is simulated directly
    // here, since this hook makes no panel calls of its own.
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, status: 200, json: () => Promise.resolve({}) }))
    watchNavigation()

    // Load 1: signed in, but nothing behind it works -- one redirect.
    const load1 = renderHook(() => useSession())
    await waitFor(() => {
      expect(load1.result.current.state).toEqual({ status: 'signed-in' })
    })
    redirectToGatewaySignIn()
    load1.unmount()
    expect(sessionStorage.getItem('fabric.gateway.redirected')).toBe('1')

    // Load 2: a fresh page load's in-memory state; the count in
    // `sessionStorage` survives, the way a real reload would leave it. The
    // same lie, the same redirect.
    resetGatewayStateForTests()
    const load2 = renderHook(() => useSession())
    await waitFor(() => {
      expect(load2.result.current.state).toEqual({ status: 'signed-in' })
    })
    redirectToGatewaySignIn()
    load2.unmount()
    expect(sessionStorage.getItem('fabric.gateway.redirected')).toBe('2')

    // Load 3: two redirects have now gone by with nothing to prove either
    // one actually worked.
    resetGatewayStateForTests()
    const navigation = watchNavigation()
    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state.status).toBe('signed-out')
    })
    expect(result.current.state).toMatchObject({ canRetry: false })
    expect((result.current.state as { error: string | null }).error).toMatch(/keeps signing you in/)
    // Reporting the loop must not itself try another redirect.
    expect(navigation.to()).toBeNull()
  })

  it('redeems a code already in the URL before the probe is even reached, so a probe that would fail cannot abandon it', async () => {
    // The property the probe's position exists to protect: a page load that
    // is the console's own provider returning must not leave the code in the
    // URL and the PKCE pair in `sessionStorage` because a later, unrelated
    // step failed.
    sessionStorage.setItem('fabric.signin.state', 'state')
    sessionStorage.setItem('fabric.signin.verifier', 'verifier')
    arriveAt('?code=code&state=state')

    const fetchMock = vi.fn((url: string) => {
      if (url.startsWith('/api/operator')) {
        return Promise.resolve({ ok: false, status: 500, json: () => Promise.resolve({}) })
      }
      return Promise.resolve({ ok: true, json: () => Promise.resolve({ access_token: 'current-token' }) })
    })
    vi.stubGlobal('fetch', fetchMock)

    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state).toEqual({ status: 'signed-in' })
    })
    expect(currentToken()).toBe('current-token')
    expect(window.location.search).toBe('')

    const requested = (fetchMock.mock.calls as [string][]).map(([url]) => url)
    expect(requested).toContain('/api/session')
  })
})
