/**
 * The console's product-catalogue requests.
 *
 * Like `./client`, every one of these calls the SaaS Fabric control-plane API
 * and nothing else, over a relative path. This file adds the catalogue,
 * client product configuration, and recorded activity to what `./client`
 * already exposes for identity and reconciliation.
 */
import { request } from './client'
import type {
  CatalogueCommand,
  ClientProductRequest,
  ClientProductResponse,
  ProductActivity,
  StoredCatalogue,
} from './catalogue-types'
import type { RuntimeCatalogue } from './runtime-catalogue-types'

/** The catalogue, and the revision to condition the next write on. */
export function getCatalogue(): Promise<StoredCatalogue> {
  return request<StoredCatalogue>('/api/catalogue')
}

/**
 * Applies one catalogue command, conditioned on the revision it was read at.
 *
 * `revision === null` means "this catalogue does not exist yet": it is sent
 * as `If-None-Match: *` rather than omitted, so a second operator creating it
 * at the same moment is refused instead of silently overwriting the first.
 * Every other write carries `If-Match` for the same reason `putIdentity`
 * does — a conflict belongs to the operator who is about to lose it, not to
 * whichever request happens to land last.
 */
export function changeCatalogue(
  command: CatalogueCommand,
  revision: string | null,
): Promise<StoredCatalogue> {
  return request<StoredCatalogue>('/api/catalogue', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(revision === null ? { 'If-None-Match': '*' } : { 'If-Match': `"${revision}"` }),
    },
    body: JSON.stringify(command),
  })
}

/** One client's resolved product state: configuration, assignments, and reconciliation. */
export function getProduct(id: string): Promise<ClientProductResponse> {
  return request<ClientProductResponse>(`/api/clients/${encodeURIComponent(id)}/product`)
}

/** Creates a client and its product configuration together, as one write. */
export function createClient(
  id: string,
  configuration: ClientProductRequest,
): Promise<ClientProductResponse> {
  return request<ClientProductResponse>('/api/clients', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id, configuration }),
  })
}

/** Replaces a client's product configuration, conditioned on the revision it was read at. */
export function saveProduct(
  id: string,
  revision: string,
  configuration: ClientProductRequest,
): Promise<ClientProductResponse> {
  return request<ClientProductResponse>(`/api/clients/${encodeURIComponent(id)}/product`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json', 'If-Match': `"${revision}"` },
    body: JSON.stringify(configuration),
  })
}

/** The platform's recorded product and identity activity. The API already sorts it newest first. */
export function getActivity(): Promise<{ activity: ProductActivity[] }> {
  return request<{ activity: ProductActivity[] }>('/api/activity')
}

/** Who the control plane believes is making these requests. */
export function getOperator(): Promise<{ subject: string }> {
  return request<{ subject: string }>('/api/operator')
}

/**
 * The derived runtime catalogue: every resource a runtime in this
 * environment would be given right now, from the newest published release
 * of whichever application declares it (ADR 0023 part 3).
 *
 * Read-only -- there is no command that writes this back. A conflict
 * between two applications declaring the same resource name, reachable only
 * through a hand edit, answers `500 desired_state_invalid`; see
 * `useRuntimeCatalogue`.
 */
export function getRuntimeCatalogue(): Promise<RuntimeCatalogue> {
  return request<RuntimeCatalogue>('/api/catalogue/runtime')
}
