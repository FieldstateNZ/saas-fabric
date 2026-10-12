import type { CurrentUser } from '../api/current-user-types'

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
