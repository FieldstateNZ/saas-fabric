/**
 * What a page load does to find out whether the gateway signed it in.
 *
 * The console holds no token and runs no sign-in of its own (ADR 0024 §2), so
 * every outcome below is decided by one probe of `/api/operator`. What matters
 * is that it never starts a flow of its own, never navigates by itself, and
 * reports a gateway it cannot help as a dead end rather than a retry.
 */
import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { useSession } from './useSession'
import { redirectToGatewaySignIn, resetGatewayStateForTests } from '../session/gateway'

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

/** A fetch answering every request with one status and body. */
function answering(status: number, body: unknown): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve(body),
  })
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

beforeEach(() => {
  sessionStorage.clear()
  resetGatewayStateForTests()
  window.history.replaceState({}, '', '/')
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('a page load', () => {
  it('signs in from the gateway session alone, asking nothing else', async () => {
    const fetchMock = answering(200, { subject: 'operator' })

    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state).toEqual({ status: 'signed-in' })
    })
    expect(fetchMock.mock.calls).toEqual([['/api/operator', expect.anything()]])
  })

  it('offers the gateway sign-in, without navigating by itself, when there is no session', async () => {
    answering(401, { error: { code: 'unauthenticated', message: 'request carries no operator identity' } })
    const navigation = watchNavigation()

    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state).toEqual({ status: 'signed-out', error: null, canRetry: true })
    })
    expect(navigation.to()).toBeNull()

    act(() => {
      result.current.signIn()
    })
    expect(navigation.to()).toBe('/')
  })

  it('never asks the control plane to redeem a code, even with one in the URL', async () => {
    window.history.replaceState({}, '', '/?code=code&state=state')
    const fetchMock = answering(401, { error: { code: 'unauthenticated', message: 'none' } })

    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state.status).toBe('signed-out')
    })
    const requested = (fetchMock.mock.calls as [string][]).map(([url]) => url)
    expect(requested).toEqual(['/api/operator'])
  })

  it('treats a control-plane failure as a real failure, not "no session"', async () => {
    answering(500, {})

    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state.status).toBe('signed-out')
    })
    expect((result.current.state as { error: string | null }).error).toMatch(/500/)
  })

  it('reports a refusal as a dead end when the control plane refuses the forwarded token', async () => {
    answering(401, { error: { code: 'operator_refused', message: 'refused' } })

    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state.status).toBe('signed-out')
    })
    expect(result.current.state).toMatchObject({ canRetry: false })
    expect((result.current.state as { error: string | null }).error).toMatch(/gateway signed you in/)
  })

  it('reports the loop rather than signing in again, once two full loads have each redirected', async () => {
    // Each load's probe runs first, and `redirectToGatewaySignIn` -- what
    // `client.ts` calls once a panel 401s -- is simulated directly here,
    // since this hook makes no panel calls of its own.
    answering(200, {})
    watchNavigation()

    const load1 = renderHook(() => useSession())
    await waitFor(() => {
      expect(load1.result.current.state).toEqual({ status: 'signed-in' })
    })
    redirectToGatewaySignIn()
    load1.unmount()

    resetGatewayStateForTests()
    const load2 = renderHook(() => useSession())
    await waitFor(() => {
      expect(load2.result.current.state).toEqual({ status: 'signed-in' })
    })
    redirectToGatewaySignIn()
    load2.unmount()
    expect(sessionStorage.getItem('fabric.gateway.redirected')).toBe('2')

    resetGatewayStateForTests()
    const navigation = watchNavigation()
    const { result } = renderHook(() => useSession())

    await waitFor(() => {
      expect(result.current.state.status).toBe('signed-out')
    })
    expect(result.current.state).toMatchObject({ canRetry: false })
    expect((result.current.state as { error: string | null }).error).toMatch(/keeps signing you in/)
    expect(navigation.to()).toBeNull()
  })
})
