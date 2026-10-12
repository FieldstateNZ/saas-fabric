/**
 * Whether a client's stored projected identity clients are still what its
 * own product configuration projects (D01-19b).
 *
 * Split from `types.ts` the way `reconciliation-types.ts` is, and
 * re-exported from it.
 */

/** Which part of a stored projected client differs. A field, never a value. */
export type ProjectedClientField = 'missing' | 'protocol' | 'pkce' | 'redirectStrategy' | 'redirectUris'

/** One stored projected client that differs from its projection. */
export interface ProjectedClientDivergence {
  /** The application whose identity client it is. */
  readonly client: string
  /** The first part found to differ. */
  readonly field: ProjectedClientField
}

/**
 * Reported on every client read. A fact about the stored document, never a
 * claim about who edited it.
 *
 * - `consistent`: every projected client is what the product configuration
 *   projects.
 * - `diverged`: these are not. A product save and a re-assertion are refused,
 *   and convergence withholds them.
 * - `unverifiable`: the projection cannot be computed from the stored
 *   document, so `unverified` names the identity clients that may be
 *   projected. Convergence withholds them.
 */
export type ProjectionCheck =
  | { readonly state: 'consistent' }
  | { readonly state: 'diverged'; readonly divergences: readonly ProjectedClientDivergence[] }
  | { readonly state: 'unverifiable'; readonly unverified: readonly string[] }
