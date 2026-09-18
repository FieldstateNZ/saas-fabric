/**
 * What the console asks about one client's data placement (ADR 0023 part 2).
 *
 * Split from `client.ts` the way `platform.ts` already is: a different
 * conversation, sharing the one `request` and the one origin every call in
 * this directory goes through.
 */
import { request } from './client'
import type { Placements } from './placement-types'

/**
 * Every logical data source a client's document asks for, and how each
 * stands.
 */
export async function getPlacements(clientId: string): Promise<Placements> {
  return request<Placements>(`/api/clients/${encodeURIComponent(clientId)}/placements`)
}

/**
 * Asks Fabric to place one logical data source's intent onto a data source
 * it selects.
 *
 * `revision` is the placements document revision this operator last read,
 * always sent as `If-Match` -- the same precondition `declareDataSource`
 * sends, and for the same reason: the late-bound binding always has a
 * generation tag to compare-and-swap on, even before any placement has ever
 * been recorded for this client. There is no request body: the intent being
 * placed is `spec.data.<logical>` on the client's own document, which the
 * control plane already has, so nothing here could add to or contradict it.
 * A refusal Fabric can name (no data source admits the intent, this logical
 * is already placed, ...) comes back as `422 placement_refused`, with the
 * refusal's own words as the message.
 */
export async function placeData(
  clientId: string,
  logical: string,
  revision: string,
): Promise<Placements> {
  return request<Placements>(
    `/api/clients/${encodeURIComponent(clientId)}/placements/${encodeURIComponent(logical)}`,
    {
      method: 'POST',
      headers: { 'If-Match': `"${revision}"` },
    },
  )
}
