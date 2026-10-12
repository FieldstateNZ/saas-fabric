/**
 * Re-asserting a client's identity projection (D01-19b).
 *
 * Split from `client.ts` the way `placements.ts` is: one more conversation,
 * sharing the one `request` every call in this directory goes through.
 */
import { request } from './client'
import type { Identity } from './types'

/**
 * Overwrites every hand edit the identity provider holds that the product
 * does not support, with the client's declared identity.
 *
 * `revision` is the version the operator reviewed, sent as `If-Match`. This
 * writes no desired state; it converges the identity provider now, as the
 * operator, and is recorded in the audit trail.
 */
export async function reassertIdentity(clientId: string, revision: string): Promise<Identity> {
  return request<Identity>(`/api/clients/${encodeURIComponent(clientId)}/identity/reassertion`, {
    method: 'POST',
    headers: { 'If-Match': `"${revision}"` },
  })
}
