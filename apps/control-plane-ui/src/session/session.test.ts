/**
 * What the sign-in round trip accepts, and what it refuses.
 *
 * The cases that matter are the ones where the callback did not come from a
 * sign-in this tab started — a stale tab, a bookmarked callback, or somebody
 * else's authorization code pasted into the address bar.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { completeSignIn, currentToken, forgetToken } from './session'

/** Puts the page at a callback URL carrying `code` and `state`. */
function arriveAt(query: string): void {
  window.history.replaceState({}, '', `/${query}`)
}

/** Pretends this tab started a sign-in and stored its half of the PKCE pair. */
function tabStarted(state: string, verifier = 'the-verifier'): void {
  sessionStorage.setItem('fabric.signin.state', state)
  sessionStorage.setItem('fabric.signin.verifier', verifier)
}

beforeEach(() => {
  sessionStorage.clear()
  forgetToken()
  arriveAt('')
})

afterEach(() => {
  vi.unstubAllGlobals()
})

/** A redemption endpoint that answers with a token. */
function redemptionSucceeds(): ReturnType<typeof vi.fn> {
  const fetched = vi.fn().mockResolvedValue({
    ok: true,
    json: () => Promise.resolve({ access_token: 'an-operator-token', expires_in: 300 }),
  })
  vi.stubGlobal('fetch', fetched)

  return fetched
}

describe('completing a sign-in', () => {
  it('does nothing when this page load is not a callback', async () => {
    const fetched = redemptionSucceeds()

    await expect(completeSignIn()).resolves.toBe(false)
    expect(fetched).not.toHaveBeenCalled()
  })

  it('redeems the code and holds the token when the state matches', async () => {
    tabStarted('the-state')
    arriveAt('?code=the-code&state=the-state')
    const fetched = redemptionSucceeds()

    await expect(completeSignIn()).resolves.toBe(true)
    expect(currentToken()).toBe('an-operator-token')

    const [, init] = fetched.mock.calls[0] as [string, { body: string }]
    expect(JSON.parse(init.body)).toEqual({
      code: 'the-code',
      code_verifier: 'the-verifier',
    })
  })

  it('refuses a callback whose state this tab did not issue', async () => {
    tabStarted('the-state')
    arriveAt('?code=someone-elses-code&state=a-different-state')
    const fetched = redemptionSucceeds()

    await expect(completeSignIn()).rejects.toThrow(/did not start here/)
    expect(fetched).not.toHaveBeenCalled()
    expect(currentToken()).toBeNull()
  })

  it('refuses a callback when this tab started no sign-in at all', async () => {
    arriveAt('?code=the-code&state=the-state')
    const fetched = redemptionSucceeds()

    await expect(completeSignIn()).rejects.toThrow(/did not start here/)
    expect(fetched).not.toHaveBeenCalled()
  })

  it('clears the code from the address bar so a reload cannot replay it', async () => {
    tabStarted('the-state')
    arriveAt('?code=the-code&state=the-state')
    redemptionSucceeds()

    await completeSignIn()

    expect(window.location.search).toBe('')
  })

  it('spends the stored verifier once, so a second attempt is refused', async () => {
    tabStarted('the-state')
    arriveAt('?code=the-code&state=the-state')
    redemptionSucceeds()

    await completeSignIn()
    arriveAt('?code=the-code&state=the-state')

    await expect(completeSignIn()).rejects.toThrow(/did not start here/)
  })

  it('holds no token when the control plane refuses the redemption', async () => {
    tabStarted('the-state')
    arriveAt('?code=an-expired-code&state=the-state')
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false, status: 401 }))

    await expect(completeSignIn()).rejects.toThrow(/could not be completed/)
    expect(currentToken()).toBeNull()
  })
})
