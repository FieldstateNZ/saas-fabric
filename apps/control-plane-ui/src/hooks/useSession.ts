/**
 * Whether the gateway has signed this operator in.
 *
 * Sign-in belongs to the gateway (ADR 0024 §2). This console holds no token
 * and runs no flow of its own, so a page load asks one question -- does
 * `/api/user/current` answer for this browser? -- and either renders the console
 * or offers to send the browser to the gateway's sign-in.
 */
import { useEffect, useState } from 'react'

import { gatewaySession, redirectToGatewaySignIn } from '../session/gateway'

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
    }
  }, [])

  return { state, signIn: redirectToGatewaySignIn }
}

/**
 * Works out where this page load stands.
 *
 * Never navigates by itself. A gateway in front of this console answers an
 * unauthenticated document load with the realm's login before the console
 * ever runs, so arriving here with no session means either that the gateway's
 * session ended between loads or that no gateway is in front at all. In the
 * second case an automatic redirect to `/` would load this page again and
 * loop; a button the operator presses cannot.
 */
async function establish(): Promise<SessionState> {
  const gateway = await gatewaySession()

  if (gateway === 'signed-in') {
    return { status: 'signed-in' }
  }
  if (gateway === 'refused') {
    // The gateway proved who this operator is; the control plane would not
    // accept the token it forwarded. Signing in again would only repeat the
    // same refusal -- this needs an operator who can fix the client or the
    // role, not another redirect.
    return {
      status: 'signed-out',
      error: 'The gateway signed you in, but the control plane refused that sign-in. Ask an operator to check the gateway client and your operator role.',
      canRetry: false,
    }
  }
  if (gateway === 'looping') {
    // The gateway keeps answering as though this browser is signed in, and
    // the control plane keeps refusing whatever it forwards. Nothing this
    // console does differently would break the cycle; only an operator fixing
    // the gateway's own configuration can.
    return {
      status: 'signed-out',
      error: 'The gateway keeps signing you in, but the control plane keeps refusing it. Ask an operator to check the gateway.',
      canRetry: false,
    }
  }

  return { status: 'signed-out', error: null, canRetry: true }
}

/** An operator-readable message for anything thrown above. */
function message(error: unknown): string {
  return error instanceof Error ? error.message : 'Signing in failed.'
}
