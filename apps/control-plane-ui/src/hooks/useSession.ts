/**
 * Whether this operator is signed in.
 *
 * Signing in is not optional any more. There used to be a second posture, where
 * a proxy asserted an identity and the console had to discover which
 * deployment it was talking to; that posture is gone, so this asks one
 * question rather than two.
 */
import { useCallback, useEffect, useState } from 'react'

import { beginSignIn, clearQuery, completeSignIn, currentToken, discardPending } from '../session/session'
import { attemptPending, callbackError, forgetAttempt, needsTheOperator, recordAttempt } from '../session/silent'

/** Where the operator stands with respect to signing in. */
export type SessionState =
  | { status: 'checking' }
  | { status: 'signed-out'; error: string | null }
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

    const settle = (next: SessionState): void => {
      if (!abandoned) {
        setState(next)
      }
    }

    void establish().then(settle, (error: unknown) => {
      settle({ status: 'signed-out', error: message(error) })
    })

    return () => {
      abandoned = true
    }
  }, [])

  const signIn = useCallback(() => {
    setState({ status: 'checking' })

    // `beginSignIn` navigates away when it succeeds, so the only path back
    // here is a failure worth showing.
    void beginSignIn().catch((error: unknown) => {
      setState({ status: 'signed-out', error: message(error) })
    })
  }, [])

  return { state, signIn }
}

/**
 * Works out where this page load stands, and signs in again if it can.
 *
 * Four outcomes, in the order they have to be checked.
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
    }
  }

  // Is this the provider returning with a code? Do this before probing, so a
  // completed sign-in never depends on a second request succeeding.
  if (await completeSignIn()) {
    forgetAttempt()
    return { status: 'signed-in' }
  }

  // Back from an attempt carrying neither a code nor an error, so something
  // ate the callback. One wasted redirect is acceptable; a loop is not.
  if (attemptPending()) {
    forgetAttempt()
    return { status: 'signed-out', error: null }
  }

  // Nothing in hand and nothing tried yet. The provider probably still holds a
  // session from earlier -- ask it, rather than asking the operator.
  recordAttempt()

  try {
    await beginSignIn({ silent: true })
  } catch (error: unknown) {
    forgetAttempt()
    return { status: 'signed-out', error: message(error) }
  }

  // `beginSignIn` navigated away; this render never lands.
  return { status: 'checking' }
}

/** An operator-readable message for anything thrown above. */
function message(error: unknown): string {
  return error instanceof Error ? error.message : 'Signing in failed.'
}
