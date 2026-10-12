/**
 * `GET /api/user/current`'s answer, as ADR 0024 §3 fixes it.
 *
 * The control plane serves it from the token the gateway forwarded and the
 * control plane verified. `roles` is the realm tier -- on this instance the
 * configured operator role -- and is a label to show, never a reason to show
 * or hide anything: every route the console calls is still authorised on the
 * server, whatever this says.
 */
export interface CurrentUser {
  /** The token's `sub`: opaque and stable, not a name to show. */
  readonly subject: string
  readonly userName: string | null
  /** Never empty: the name, else the username, else the subject. */
  readonly displayName: string
  readonly email: string | null
  readonly roles: readonly string[]
}
