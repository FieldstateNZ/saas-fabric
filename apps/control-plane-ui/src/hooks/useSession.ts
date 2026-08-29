/**
 * Whether this operator is signed in.
 *
 * Signing in is not optional any more. There used to be a second posture, where
 * a proxy asserted an identity and the console had to discover which
 * deployment it was talking to; that posture is gone, so this asks one
 * question rather than two.
 */
import { useCallback, useEffect, useState } from 'react'

import { beginSignIn, completeSignIn, currentToken } from '../session/session'

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

/** Works out where this page load stands. */
async function establish(): Promise<SessionState> {
  if (currentToken() !== null) {
    return { status: 'signed-in' }
  }

  // Is this the provider returning with a code? Do this before probing, so a
  // completed sign-in never depends on a second request succeeding.
  if (await completeSignIn()) {
    return { status: 'signed-in' }
  }

  return { status: 'signed-out', error: null }
}

/** An operator-readable message for anything thrown above. */
function message(error: unknown): string {
  return error instanceof Error ? error.message : 'Signing in failed.'
}
