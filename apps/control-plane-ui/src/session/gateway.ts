/**
 * Whether the gateway has signed this browser in.
 *
 * ADR 0024: sign-in is not this console's job. The gateway runs it, holds the
 * result in HMAC-signed cookies the browser sends on every request, and
 * forwards the access token upstream as a bearer. The console holds no token,
 * generates no PKCE pair and calls no sign-in route of its own; it asks this
 * probe whether a session exists and, when one does not, navigates to the
 * gateway's sign-in.
 *
 * # Why `/api/user/current`
 *
 * It is the route ADR 0024 §3 gives every instance's shell for "who is
 * signed in", and on this instance it answers only for a verified operator:
 * no bearer is `401 unauthenticated`, a refused one `401 operator_refused`,
 * exactly as every other route. A `200` must also carry a complete
 * `CurrentUser` (`api/current-user-types.ts`); anything else is thrown
 * rather than read as a session.
 * Asking it costs nothing *irreversible*: no navigation is risked.
 * It is not free of side effects any more, though -- a
 * `200` drops the query string, the way completing a redirect always has, and
 * every call consumes a stale marker this file itself may have left. See
 * `gatewaySession` for both.
 *
 * # Why a non-401/403 failure throws
 *
 * A `500` or a network failure means the control plane is unreachable, not
 * "nobody is signed in." Treating it as "no session" would send an operator
 * whose session is perfectly fine back through the gateway's sign-in, which
 * cannot fix an unreachable control plane. Surfacing the failure is the
 * honest answer; `useSession` turns it into a message.
 *
 * # A duplicated tab
 *
 * `sessionStorage` is per tab but a duplicated tab inherits a copy of it --
 * the same way it inherits any other `sessionStorage` entry. Two tabs can each hold the redirect count below and each report
 * `'refused'` or `'looping'` independently; that is a correct answer twice,
 * not a bug, because each tab genuinely did redirect and each genuinely got
 * the same answer back.
 *
 * # A lying upstream
 *
 * Nothing here can tell a gateway that verified this browser from a proxy
 * that answers `200` to `/api/user/current` without checking anything -- that
 * trust decision is Envoy's `SecurityPolicy` and the control plane's own
 * bearer verification, upstream of every line in this file. This module
 * cannot close that gap; authorisation itself is the platform's boundary,
 * not this probe's. What it *can* do is stop repeating the mistake: behind a
 * proxy that lies, every *real* request still `401`s because
 * there is no bearer behind the lie, `redirectToGatewaySignIn` sends the
 * browser away, and the gateway (still lying) sends it straight back to a
 * probe that says `200` again. That `200` alone proves nothing -- a lying
 * proxy can produce it forever -- so it no longer clears the count on its
 * own; only [`clearRedirectCount`] does, and `client.ts` calls that on the
 * first response any *other* route gives that is not a `401`, which a lying
 * upstream can never produce because there is still no real bearer behind
 * it. Traced across page loads: load one redirects once (count `1`), load
 * two's probe still says `200` and still redirects (count `2`), and load
 * three's probe finds the count already at [`LOOP_THRESHOLD`] and answers
 * `'looping'` instead of trying a third time -- `useSession` renders that as
 * a dead end with no retry rather than another silent redirect. A genuine
 * one-off hiccup looks different from the inside: its first *real* request
 * after the one redirect succeeds, `client.ts` clears the count right then,
 * and whatever ends that session much later starts from zero rather than
 * being held to a coincidence that happened once.
 */
import { readCurrentUser } from './current-user'
import { clearQuery, rememberDestination } from './destination'

/** Set while this tab is navigating to the gateway's sign-in, so a second
 *  401 arriving before the browser actually leaves does not navigate again. */
let navigating = false

/** How many redirects in a row have not yet been proven unnecessary by a
 *  real request succeeding -- the belt for a `401` whose body cannot say
 *  why, and the count `'looping'` is judged against. Cleared by
 *  `clearRedirectCount`, not by a mere `200` from the probe (see the module
 *  doc's "A lying upstream"). Survives navigation in `sessionStorage`; the
 *  key name outlives what it once held (a boolean marker), which is now a
 *  count. */
const REDIRECT_COUNT = 'fabric.gateway.redirected'

/** A `200` arriving with this many redirects still uncleared is not a
 *  session recovering -- it is the same refusal, again. */
const LOOP_THRESHOLD = 2

/** Set once a real request has confirmed, this page load, that the session
 *  actually works -- so `clearRedirectCount`, which `client.ts` calls on
 *  every non-`401` response, only touches `sessionStorage` the first time. */
let redirectCountConfirmed = false

/** The route that says whether this browser is signed in (ADR 0024 §3). */
const PROBE = '/api/user/current'

/** What the probe found. */
export type GatewayProbe = 'signed-in' | 'no-session' | 'refused' | 'looping'

/** The redirect count `sessionStorage` currently holds, or `0`. */
function redirectCount(): number {
  const stored = sessionStorage.getItem(REDIRECT_COUNT)
  const count = stored === null ? 0 : Number.parseInt(stored, 10)
  return Number.isNaN(count) ? 0 : count
}

/**
 * Asks whether an upstream has already signed this browser in.
 *
 * `'signed-in'` on a `200` carrying a complete `CurrentUser` (a `200` that
 * does not carry one is thrown), having dropped the query string the same way
 * `clearQuery` always does -- deliberately, on every such `200`, whether or
 * not anything was in it, because this call does not know in advance
 * whether it is answering a fresh tab or one returning from a redirect. The
 * redirect count is *not* cleared just because this probe says `200` -- a
 * proxy that lies about `/api/user/current` can say `200` forever, so only
 * [`clearRedirectCount`] clears it, on evidence a real request actually
 * worked. `'looping'` on a `200` that arrives with the count already at
 * [`LOOP_THRESHOLD`] -- see the module doc's "A lying upstream". `'refused'`
 * on `401` when the body names `operator_refused`: the control plane's own
 * word that a bearer was presented and rejected, which needs no count to
 * trust. The count is the belt for a `401` whose body cannot be read at all
 * -- consulted only then, and cleared on every `401`/`403` and every
 * non-`200` throw, so it
 * never outlives the one probe that should read it for those. `'no-session'`
 * on any other `401`/`403`. Anything else is thrown.
 */
export async function gatewaySession(): Promise<GatewayProbe> {
  // `same-origin` is `fetch`'s own default; named here anyway so a reader
  // checking what this probe can and cannot leak does not have to already
  // know that, on the one call in this file that decides whether a cookie
  // session exists at all.
  const response = await fetch(PROBE, { credentials: 'same-origin' })
  const count = redirectCount()

  if (response.ok) {
    if (count >= LOOP_THRESHOLD) {
      // Left exactly as it is: the count must survive a `'looping'` outcome,
      // or the very next probe would read zero and call this recovered.
      return 'looping'
    }

    // Read before anything is cleared: a `200` that is not a signed-in user
    // is not a session, and must not consume the query or the count.
    const user = readCurrentUser(await response.json().catch(() => null))
    if (user === null) {
      throw new Error(`The control plane answered ${PROBE} without a signed-in user.`)
    }

    clearQuery()
    return 'signed-in'
  }

  // Every non-200 outcome consumes the count -- a probe that fails partway
  // through (the throw below) must not leave it for the *next* probe to
  // misread, and a `401`/`403` has already read it for its own purposes.
  sessionStorage.removeItem(REDIRECT_COUNT)

  if (response.status === 401) {
    const code = await refusalCode(response)
    return code === 'operator_refused' || (code === null && count > 0) ? 'refused' : 'no-session'
  }

  if (response.status === 403) {
    return 'no-session'
  }

  throw new Error(`The control plane answered ${String(response.status)} for ${PROBE}.`)
}

/**
 * Clears the redirect count because a real request just proved the session
 * works -- a `200` from `/api/user/current` alone cannot prove that (see the
 * module doc's "A lying upstream"). `client.ts` calls this on every response
 * that is not a `401`; only the first call each page load does anything.
 */
export function clearRedirectCount(): void {
  if (redirectCountConfirmed) return
  redirectCountConfirmed = true
  sessionStorage.removeItem(REDIRECT_COUNT)
}

/**
 * The `error.code` a `401`'s body names, or `null` if it cannot be read.
 *
 * `null` is not "not refused" -- it is "unknown", which is exactly the case
 * the redirect count exists to cover instead.
 */
async function refusalCode(response: Response): Promise<string | null> {
  try {
    const body = (await response.json()) as { error?: { code?: unknown } }
    return typeof body.error?.code === 'string' ? body.error.code : null
  } catch {
    return null
  }
}

/**
 * Sends the browser to the gateway's sign-in, at most once per tab.
 *
 * `client.ts` calls this for every `401` a panel receives, and several can
 * arrive in the same tick -- one navigation must win, not one per panel.
 * `navigating` makes every call after the first a no-op; nothing in this
 * module resets it, because the only way it should stop mattering is the
 * navigation it guards actually leaving the page (`resetGatewayStateForTests`
 * below exists only because a test suite does not leave the page). The count
 * it increments only for an actual redirect -- not for a call the guard
 * turns away -- is the belt `gatewaySession` falls back to when a `401`'s
 * body cannot be read, and the count `'looping'` is judged against. The
 * current hash route is remembered before it, so returning here does not
 * lose the operator's place. `/` is a document navigation the gateway
 * answers with the realm's login when it holds no session.
 */
export function redirectToGatewaySignIn(): void {
  if (navigating) return
  navigating = true

  sessionStorage.setItem(REDIRECT_COUNT, String(redirectCount() + 1))
  rememberDestination()
  window.location.assign('/')
}

/**
 * Test-only: clears every flag this module holds in memory.
 *
 * A real page load never needs this -- each flag exists to be true for the
 * rest of that page's life. A test file does not get a fresh page between
 * `it`s, so without this, whichever test first observes a gateway session,
 * navigates, or confirms a real request leaves that true for every test that
 * runs after it in the same file. Never called from product code; nothing
 * here clears `sessionStorage`, because that already resets the way tests
 * here reset it -- per test, via `sessionStorage.clear()`.
 */
export function resetGatewayStateForTests(): void {
  navigating = false
  redirectCountConfirmed = false
}
