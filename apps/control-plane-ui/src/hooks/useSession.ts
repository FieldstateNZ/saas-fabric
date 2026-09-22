/**
 * Whether this operator is signed in.
 *
 * Signing in is not optional any more. There used to be a second posture, where
 * a proxy asserted an identity and the console had to discover which
 * deployment it was talking to; that posture is gone, so this asks one
 * question rather than two.
 */
import { useCallback, useEffect, useState } from 'react'

import { gatewaySession } from '../session/gateway'
import { beginSignIn, clearQuery, completeSignIn, currentToken, discardPending, onSessionEnded } from '../session/session'
import { attemptPending, callbackError, forgetAttempt, needsTheOperator, recordAttempt } from '../session/silent'

/** Where the operator stands with respect to signing in. */
export type SessionState =
  | { status: 'checking' }
  | { status: 'signed-out'; error: string | null; canRetry: boolean }
  | { status: 'signed-in' }

/** The session state, and the action that starts a sign-in. */
export interface Session {
  readonly state: SessionState
  readonly signIn: () => void
}

export function useSession(): Session {
  const [state, setState] = useState<SessionState>({ status: 'checking' })

  useEffect(() => {
    let abandoned = false
    const unsubscribe = onSessionEnded(() => {
      if (abandoned) return
      abandoned = true
      setState({ status: 'signed-out', error: 'Your session ended. Sign in again to continue.', canRetry: true })
    })

    const settle = (next: SessionState): void => {
      if (!abandoned) {
        setState(next)
      }
    }

    void establish().then(settle, (error: unknown) => {
      settle({ status: 'signed-out', error: message(error), canRetry: true })
    })

    return () => {
      abandoned = true
      unsubscribe()
    }
  }, [])

  const signIn = useCallback(() => {
    setState({ status: 'checking' })

    // `beginSignIn` navigates away when it succeeds, so the only path back
    // here is a failure worth showing.
    void beginSignIn().catch((error: unknown) => {
      setState({ status: 'signed-out', error: message(error), canRetry: true })
    })
  }, [])

  return { state, signIn }
}

/**
 * Works out where this page load stands, and signs in again if it can.
 *
 * Already holding a token ends this immediately. Otherwise, in order: a
 * callback error, a code to redeem, a session the gateway already holds, an
 * attempt already spent, and -- only then -- a new silent attempt.
 *
 * The gateway probe sits after redeeming a code, not before everything. A
 * page load that is the *console's own* provider returning from its PKCE
 * flow must redeem that code and clear the query string no matter what the
 * probe says or whether it can be reached at all -- otherwise a probe
 * failure (a `500`, say) would leave the authorization code sitting in the
 * URL and the PKCE pair sitting in `sessionStorage`, exactly the leftover
 * state `discardPending`/`clearQuery` exist to prevent. The probe only has
 * to run ahead of the one step it exists to prevent: the silent attempt,
 * which would otherwise start a second sign-in over a session the gateway
 * already holds. It also has to precede the pending-attempt check below,
 * for the same reason -- a page load behind the gateway never made a silent
 * attempt of its own, so that check must not answer for it.
 */
async function establish(): Promise<SessionState> {
  if (currentToken() !== null) {
    return { status: 'signed-in' }
  }

  // The provider refusing a silent attempt. Usually it simply has no session
  // for this browser, which is not a fault and is not worth alarming anybody
  // about -- the button below says the rest.
  const refusal = callbackError()
  if (refusal !== null) {
    forgetAttempt()
    discardPending()
    clearQuery()

    return {
      status: 'signed-out',
      error: needsTheOperator(refusal) ? null : `Signing in failed (${refusal}).`,
      canRetry: true,
    }
  }

  // Is this the provider returning with a code? Do this before probing, so a
  // completed sign-in never depends on a second request succeeding.
  if (await completeSignIn()) {
    forgetAttempt()
    return { status: 'signed-in' }
  }

  const gateway = await gatewaySession()
  if (gateway === 'signed-in') {
    return { status: 'signed-in' }
  }
  if (gateway === 'refused') {
    // The gateway proved who this operator is; the control plane would not
    // accept the token it forwarded. Retrying silently would only repeat
    // the same refusal -- this needs an operator who can fix the client or
    // the role, not another redirect. Cleaned up the same way a callback
    // error is above: whatever silent-attempt or PKCE state this tab was
    // carrying is stale now and must not confuse whatever comes next.
    forgetAttempt()
    discardPending()

    return {
      status: 'signed-out',
      error: 'The gateway signed you in, but the control plane refused that sign-in. Ask an operator to check the gateway client and your operator role.',
      // `SignIn`'s only retry is `beginSignIn`, the standalone PKCE flow --
      // not the flow that just ended, and not one that would do anything
      // differently. Offering it would offer a button that cannot help.
      canRetry: false,
    }
  }
  if (gateway === 'looping') {
    // The gateway keeps answering as though this browser is signed in, and
    // the control plane keeps refusing whatever it forwards -- the same
    // cycle `'refused'` handles once, now caught repeating. Nothing this
    // console does differently would break it; only an operator fixing the
    // gateway's own configuration can.
    forgetAttempt()
    discardPending()

    return {
      status: 'signed-out',
      error: 'The gateway keeps signing you in, but the control plane keeps refusing it. Ask an operator to check the gateway.',
      canRetry: false,
    }
  }

  // Back from an attempt carrying neither a code nor an error, so something
  // ate the callback. One wasted redirect is acceptable; a loop is not.
  if (attemptPending()) {
    forgetAttempt()
    return { status: 'signed-out', error: null, canRetry: true }
  }

  // Nothing in hand and nothing tried yet. The provider probably still holds a
  // session from earlier -- ask it, rather than asking the operator.
  recordAttempt()

  try {
    await beginSignIn({ silent: true })
  } catch (error: unknown) {
    forgetAttempt()
    return { status: 'signed-out', error: message(error), canRetry: true }
  }

  // `beginSignIn` navigated away; this render never lands.
  return { status: 'checking' }
}

/** An operator-readable message for anything thrown above. */
function message(error: unknown): string {
  return error instanceof Error ? error.message : 'Signing in failed.'
}
