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
  readonly onSignIn: () => void
}

export function SignIn({ error, onSignIn }: SignInProps) {
  return (
    <div className="signin">
      <p className="signin__title">SaaS Fabric</p>
      <h1 className="signin__heading">Operator console</h1>
      <p className="signin__lead">
        Administering this platform needs an operator identity. Signing in takes you to the
        platform&rsquo;s identity provider and back.
      </p>

      {error !== null && <p className="error">{error}</p>}

      <button type="button" className="signin__button" onClick={onSignIn}>
        Sign in
      </button>
    </div>
  )
}
