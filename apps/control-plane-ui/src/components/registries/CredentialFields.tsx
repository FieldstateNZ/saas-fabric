import { Field } from '../../product/Field'

/**
 * A registry credential's two fields: the account, and its token.
 *
 * # The token is typed once and never shown again
 *
 * It is a password input, so it is masked as it is typed. A username beside
 * a password reads to a browser as this site's sign-in form, so each says
 * otherwise: the token is `new-password`, which browsers honour where they
 * ignore `off` on a password -- it is not filled with a password saved for
 * the console, which would then go to a registry's realm -- and the
 * username is `off`. A browser may still offer to save a new password; the
 * console cannot forbid that, only never fill one in. The form that owns it
 * empties it
 * the moment it is sent, whatever the answer: the control plane never
 * returns a token -- there is no reveal -- so nothing on this page could
 * put it back on screen, and a failed attempt means typing it again rather
 * than a secret kept in a page for longer than one request.
 */
export function CredentialFields({
  username,
  token,
  required,
  onUsername,
  onToken,
}: {
  readonly username: string
  readonly token: string
  readonly required: boolean
  readonly onUsername: (username: string) => void
  readonly onToken: (token: string) => void
}) {
  return (
    <>
      <Field
        label="Username"
        value={username}
        required={required}
        onChange={onUsername}
        autoComplete="off"
        hint="The registry account the token belongs to."
      />
      <label className="form-field">
        <span>
          Token
          {required && ' *'}
        </span>
        <input
          type="password"
          autoComplete="new-password"
          value={token}
          required={required}
          maxLength={4096}
          onChange={(event) => {
            onToken(event.target.value)
          }}
        />
        <small>A long-lived token. Kept by the control plane and never shown again.</small>
      </label>
    </>
  )
}
