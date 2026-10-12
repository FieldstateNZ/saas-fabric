/**
 * Whether a client's identity has taken effect.
 *
 * Split from `types.ts` the way `lifecycle-types.ts` is, and re-exported
 * from it, so every existing import keeps working.
 */

/** Where a client's identity stands between what Git says and what exists. */
export type ReconciliationStatus = 'pending' | 'applied' | 'failed' | 'drifted' | 'handEdited'

/**
 * A hand edit made in the identity provider that the product does not
 * support, and which reconciliation reported and left in place (D01-19b).
 */
export interface HandEdit {
  /** The application client that carries it. */
  readonly client: string
  /** What was edited. */
  readonly field: 'clientDisabled' | 'standardFlowDisabled' | 'declaredRedirectUriRemoved'
}

/** What is known about whether a client's desired state has taken effect. */
export interface Reconciliation {
  readonly status: ReconciliationStatus
  /** Seconds since the Unix epoch, or null if it has never been established. */
  readonly observedAtUnix: number | null
  /** Why the last attempt failed, if it did. */
  readonly detail: string | null
  /** Hand edits left in place; non-empty exactly when `status` is `handEdited`. */
  readonly handEdits: readonly HandEdit[]
}
