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

/**
 * The body as a {@link CurrentUser}, or `null` if it is not a complete one.
 *
 * Complete means every field present -- `null` where the contract allows it,
 * never omitted -- so a proxy's own `200` page, or a route answering some
 * other shape, does not read as a signed-in user.
 */
export function readCurrentUser(body: unknown): CurrentUser | null {
  if (typeof body !== 'object' || body === null) return null
  const candidate = body as Record<string, unknown>
  const { subject, userName, displayName, email, roles } = candidate

  const complete =
    nonBlank(subject) &&
    nonBlank(displayName) &&
    'userName' in candidate &&
    nullableText(userName) &&
    'email' in candidate &&
    nullableText(email) &&
    Array.isArray(roles) &&
    roles.length > 0 &&
    roles.every(nonBlank)

  return complete ? { subject, userName, displayName, email, roles } : null
}

function nonBlank(value: unknown): value is string {
  return typeof value === 'string' && value.trim() !== ''
}

function nullableText(value: unknown): value is string | null {
  return value === null || typeof value === 'string'
}
