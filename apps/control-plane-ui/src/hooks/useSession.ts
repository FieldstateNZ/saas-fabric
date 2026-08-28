/**
 * Whether this operator is signed in, and whether they need to be.
 *
 * # Both postures, decided by the API rather than by a build flag
 *
 * A deployment either signs operators in against the platform's identity
 * provider or consumes an identity its operator-plane proxy established. The
 * console does not need to be built differently for the two: the sign-in
 * routes exist only in the first, so a `404` from `/api/session` *is* the
 * answer, and it comes from the deployment rather than from an environment
 * variable somebody has to keep in step.
 */
import { useCallback, useEffect, useState } from 'react'

import { beginSignIn, completeSignIn, currentToken } from '../session/session'

/** Where the operator stands with respect to signing in. */
export type SessionState =
  | { status: 'checking' }
  /** This deployment authenticates at the network boundary; nothing to do. */
  | { status: 'not-required' }
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

  const response = await fetch('/api/session')

  // Not mounted: this deployment has no sign-in, so the operator is already
  // whoever the operator-plane proxy says they are.
  if (response.status === 404) {
    return { status: 'not-required' }
  }

  return { status: 'signed-out', error: null }
}

/** An operator-readable message for anything thrown above. */
function message(error: unknown): string {
  return error instanceof Error ? error.message : 'Signing in failed.'
}
