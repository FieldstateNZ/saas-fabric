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

/** `/api/session` answering with somewhere to sign in. */
function providerConfigured(): void {
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          authorization_endpoint: 'https://identity.example/auth',
          client_id: 'saas-fabric-console',
          redirect_uri: 'https://console.example/',
          scope: 'openid',
        }),
    }),
  )
}

beforeEach(() => {
  sessionStorage.clear()
  forgetToken()
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
      expect(result.current.state).toEqual({ status: 'signed-out', error: null })
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
      expect(result.current.state).toEqual({ status: 'signed-out', error: null })
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
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true,
      json: () => Promise.resolve({ access_token: 'current-token' }) }))
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
    expect(result.current.state).toEqual({ status: 'signed-out', error: 'Your session ended. Sign in again to continue.' })
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
