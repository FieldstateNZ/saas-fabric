/**
 * Publishing the runtime's three documents now, rather than waiting for the
 * schedule.
 *
 * Split from `platform.ts` because that file is already at its own settled
 * length (147 lines); this is one call, for one reason, and belongs beside
 * the shapes it speaks in `publication-types.ts` rather than pushing the
 * neighbour over the line-count policy.
 */
import { request } from './client'
import type { Publication } from './publication-types'

/**
 * Runs a publication pass now, as this operator, and answers with the row
 * built from the pass it just ran.
 *
 * A `POST` because it is an act, not a resource being replaced — the same
 * reasoning `converge` and `rollBackComponent` follow. No body: what gets
 * composed and offered is the platform's to decide, never something a
 * request names. The response is rendered directly by the caller rather than
 * triggering a reload: it already *is* the row a fresh `GET /api/platform`
 * would report for this exact pass, so re-reading would only risk describing
 * a later one instead.
 */
export async function publishRuntimeState(): Promise<Publication> {
  return request<Publication>('/api/platform/publication', { method: 'POST' })
}
