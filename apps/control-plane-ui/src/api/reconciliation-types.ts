/**
 * Whether a client's identity has taken effect.
 *
 * Split from `types.ts` the way `lifecycle-types.ts` is, and re-exported
 * from it, so every existing import keeps working.
 */

/** Where a client's identity stands between what Git says and what exists. */
export type ReconciliationStatus = 'pending' | 'applied' | 'failed' | 'drifted' | 'handEdited' | 'withheld'

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
  /** Hand edits left in place; non-empty when `status` is `handEdited`, and may be beside `withheld`. */
  readonly handEdits: readonly HandEdit[]
  /**
   * Projected application clients the last pass did not converge, because
   * their stored entry is not their projection (D01-19b). Non-empty exactly
   * when `status` is `withheld`.
   */
  readonly withheld: readonly string[]
}
