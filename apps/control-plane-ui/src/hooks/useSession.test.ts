/**
 * What a signed-out page load does before it gives up and asks.
 *
 * The token is held in memory, so every refresh, every new tab and both round
 * trips through GitHub arrive here with nothing in hand. What matters is that
 * the console asks the provider before it asks the operator — and that it does
 * so at most once, because a console that bounces off its identity provider
 * forever is worse than one that shows a button.
 */
import { renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { forgetToken } from '../session/session'
import { useSession } from './useSession'

/** Puts the page at a URL, without navigating. */
function arriveAt(query: string): void {
  window.history.replaceState({}, '', `/${query}`)
}

/** Captures where the console tried to send the browser. */
function watchNavigation(): { to: () => string | null } {
  let destination: string | null = null
  vi.stubGlobal('location', { ...window.location, assign: (url: string) => { destination = url } })

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
