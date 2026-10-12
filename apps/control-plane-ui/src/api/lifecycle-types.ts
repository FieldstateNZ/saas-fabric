/**
 * Where a client stands (D01-14, D01-19a).
 *
 * `incomplete` is saved intent: the document exists and the client is not
 * usable yet. `ready` means provisioning, identity and publication are all
 * met. `tombstoned` means retirement is recorded, with state retained;
 * `deletionConfirmed` records that deletion was confirmed. Neither removes
 * anything, and neither on its own means the client stopped being served.
 */
export type LifecycleState = 'incomplete' | 'ready' | 'tombstoned' | 'deletionConfirmed'

/** A readiness condition a saved client has not met yet. */
export interface UnmetCondition {
  readonly condition: 'provisioning' | 'identity' | 'publication'
  /** Why, in an operator's words. */
  readonly detail: string
}

/**
 * What publication evidence shows about a retired client's tenant.
 *
 * Withdrawal is not built, so there is no "withdrawn" state: `pending` means
 * the accepted tenants document still carries the tenant, `refused` that the
 * last pass was refused or failed and the last good documents stay,
 * `unobserved` that nothing shows what is published, and `notPublished` that
 * the accepted document does not carry it (no reader is observed applying it).
 */
export interface Withdrawal {
  readonly state: 'pending' | 'refused' | 'unobserved' | 'notPublished'
  readonly detail: string
}

/** A recorded tombstone. */
export interface Tombstone {
  readonly at: number
  readonly by: string
  readonly reason: string
}

/**
 * A tombstone an operator reversed (D01-14 restore). The tombstone is kept:
 * restoring a client never erases that it was retired.
 */
export interface Restoration {
  readonly tombstone: Tombstone
  readonly at: number
  readonly by: string
  readonly reason: string
}

/** A client's lifecycle, as the API derives it on every read. */
export interface ClientLifecycle {
  readonly state: LifecycleState
  /** Empty unless the state is `incomplete`. */
  readonly unmet: readonly UnmetCondition[]
  readonly tombstone: Tombstone | null
  readonly deletionConfirmed: { readonly at: number; readonly by: string } | null
  /** Null unless retirement is recorded. */
  readonly withdrawal: Withdrawal | null
  /** Every reversed tombstone, oldest first, in every state. */
  readonly restorations: readonly Restoration[]
}
