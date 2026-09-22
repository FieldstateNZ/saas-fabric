import { Brand } from '../console/Brand'

/**
 * The screen an operator sees before they have signed in.
 *
 * Deliberately says nothing about which identity provider is behind the
 * button. An operator signs in to SaaS Fabric; which service verifies them is
 * the platform's business, and naming it here would be the same leak the rest
 * of this console avoids (section 17).
 */
interface SignInProps {
  readonly error: string | null
  /** Whether the button belongs on screen at all.
   *
   *  `false` for the one state where offering it would be dishonest: the
   *  gateway already signed this operator in and the control plane refused
   *  that sign-in, so the console's own sign-in flow is not the flow that
   *  ended and clicking it could not fix anything -- the message already
   *  says what will (ask an operator to check the client or the role). */
  readonly canRetry: boolean
  readonly onSignIn: () => void
}

export function SignIn({ error, canRetry, onSignIn }: SignInProps) {
  return (
    <div className="signin">
      <p className="signin__title"><Brand /></p>
      <h1 className="signin__heading">Operator console</h1>
      <p className="signin__lead">
        Sign in with your operator identity. Your organisation&rsquo;s sign-in handles the rest.
      </p>

      {error !== null && <p className="error">{error}</p>}

      {canRetry && (
        <button type="button" className="signin__button" onClick={onSignIn}>
          Sign in
        </button>
      )}
    </div>
  )
}
