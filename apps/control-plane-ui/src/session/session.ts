/**
 * Signing the operator in, and holding the token that results.
 *
 * # The console never meets the identity provider
 *
 * It *navigates* the browser there — a top-level navigation, which the
 * console's `default-src 'self'` policy does not govern — and the control-plane
 * API tells it where to go. Redeeming the code is a call to this platform's own
 * API, which exchanges it server-side. So this file names no provider, holds no
 * client secret, and the console image carries no deployment's identity
 * configuration.
 *
 * # The token lives in memory only
 *
 * Not `localStorage`, not a cookie. A reload signs in again, which costs one
 * redirect that the provider's own session makes invisible. The alternative is
 * a token that outlives the tab in a place every script on the page can read.
 */
import { generate, randomState } from './pkce'

/** Where the verifier and state wait for the redirect to come back. */
const VERIFIER = 'fabric.signin.verifier'
const STATE = 'fabric.signin.state'

/** The operator's token, for as long as this tab is open. */
let token: string | null = null

/** What the API says about where to sign in. */
interface SessionConfig {
  authorization_endpoint: string
  client_id: string
  redirect_uri: string
  scope: string
}

/** The token the operator currently holds, if any. */
export function currentToken(): string | null {
  return token
}

/** Forgets the token, so the next render asks the operator to sign in. */
export function forgetToken(): void {
  token = null
}

/**
 * Sends the browser to the identity provider.
 *
 * Does not return: the page navigates away.
 */
export async function beginSignIn(): Promise<void> {
  const response = await fetch('/api/session')
  if (!response.ok) {
    throw new Error('The control plane could not say where to sign in.')
  }

  const config = (await response.json()) as SessionConfig
  const { verifier, challenge } = await generate()
  const state = randomState()

  sessionStorage.setItem(VERIFIER, verifier)
  sessionStorage.setItem(STATE, state)

  const query = new URLSearchParams({
    response_type: 'code',
    client_id: config.client_id,
    redirect_uri: config.redirect_uri,
    scope: config.scope,
    state,
    code_challenge: challenge,
    code_challenge_method: 'S256',
  })

  window.location.assign(`${config.authorization_endpoint}?${query.toString()}`)
}

/**
 * Completes a sign-in if this page load is the provider returning.
 *
 * Returns whether a token was obtained. The query string is cleared either
 * way, so a reload cannot replay a spent code and an error does not persist in
 * a URL the operator might share.
 */
export async function completeSignIn(): Promise<boolean> {
  const query = new URLSearchParams(window.location.search)
  const code = query.get('code')
  const returnedState = query.get('state')

  if (code === null || returnedState === null) {
    return false
  }

  const expectedState = sessionStorage.getItem(STATE)
  const verifier = sessionStorage.getItem(VERIFIER)
  sessionStorage.removeItem(STATE)
  sessionStorage.removeItem(VERIFIER)
  clearQuery()

  // A callback this tab did not start. Refused rather than redeemed: it is
  // either a stale tab or somebody else's authorization code.
  if (expectedState === null || verifier === null || returnedState !== expectedState) {
    throw new Error('That sign-in did not start here. Try signing in again.')
  }

  const response = await fetch('/api/session', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ code, code_verifier: verifier }),
  })

  if (!response.ok) {
    throw new Error('The sign-in could not be completed. Try again.')
  }

  const issued = (await response.json()) as { access_token: string }
  token = issued.access_token

  return true
}

/** Removes the authorization code from the address bar. */
function clearQuery(): void {
  window.history.replaceState({}, '', window.location.pathname)
}
