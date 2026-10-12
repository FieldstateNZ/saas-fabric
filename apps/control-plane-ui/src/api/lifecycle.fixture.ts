import type { ClientLifecycle } from './lifecycle-types'

/** A client the API reports as saved intent, for test fixtures. */
export const SAVED_INTENT: ClientLifecycle = {
  state: 'incomplete',
  unmet: [
    { condition: 'provisioning', detail: 'the client declares no data to place' },
    { condition: 'identity', detail: 'identity has not been applied at this revision yet' },
    { condition: 'publication', detail: 'this deployment publishes no runtime documents' },
  ],
  tombstone: null,
  deletionConfirmed: null,
  withdrawal: null,
  restorations: [],
  continuity: { state: 'admitted', reason: null, detail: null },
}
