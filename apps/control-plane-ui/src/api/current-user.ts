/**
 * Who the control plane says is signed in (ADR 0024 §3).
 *
 * The same route the gateway probe asks, read here for display. A `200`
 * that is not a complete `CurrentUser` is refused rather than shown, for the
 * same reason the probe refuses it.
 */
import { readCurrentUser } from '../session/current-user'
import { request } from './client'
import type { CurrentUser } from './current-user-types'

export async function getCurrentUser(): Promise<CurrentUser> {
  const user = readCurrentUser(await request<unknown>('/api/user/current'))
  if (user === null) {
    throw new Error('The control plane answered /api/user/current without a signed-in user.')
  }
  return user
}
