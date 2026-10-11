/**
 * Where a client stands (D01-14, D01-19a).
 *
 * `incomplete` is saved intent: the document exists and the client is not
 * usable yet. `ready` means provisioning, identity and publication are all
 * met. `tombstoned` is taken out of service with its state retained, and
 * `deletionConfirmed` records that deletion was confirmed — nothing is
 * deleted by it.
 */
export type LifecycleState = 'incomplete' | 'ready' | 'tombstoned' | 'deletionConfirmed'

/** A readiness condition a saved client has not met yet. */
export interface UnmetCondition {
  readonly condition: 'provisioning' | 'identity' | 'publication'
  /** Why, in an operator's words. */
  readonly detail: string
}

/** A client's lifecycle, as the API derives it on every read. */
export interface ClientLifecycle {
  readonly state: LifecycleState
  /** Empty unless the state is `incomplete`. */
  readonly unmet: readonly UnmetCondition[]
  readonly tombstone: { readonly at: number; readonly by: string; readonly reason: string } | null
  readonly deletionConfirmed: { readonly at: number; readonly by: string } | null
}
