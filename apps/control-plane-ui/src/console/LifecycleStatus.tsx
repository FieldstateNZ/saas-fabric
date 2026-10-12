import type { ClientLifecycle, LifecycleState, Withdrawal } from '../api/lifecycle-types'
import { Status } from './Status'

/** The words an operator sees for each state. */
const LABEL: Record<LifecycleState, string> = {
  incomplete: 'Saved, not usable yet',
  ready: 'Ready',
  tombstoned: 'Retirement recorded',
  deletionConfirmed: 'Deletion confirmed (intent)',
}

/** How each withdrawal state reads. None of them says the client stopped being served. */
const WITHDRAWAL: Record<Withdrawal['state'], string> = {
  pending: 'Withdrawal pending: still published',
  refused: 'Withdrawal not applied: publication refused',
  unobserved: 'Withdrawal not observed',
  notPublished: 'Not in the published tenants document',
}

/** How each continuity hold reads. */
const HELD: Record<Exclude<ClientLifecycle['continuity']['reason'], null>, string> = {
  noStore: 'Lifecycle not confirmed: no continuity store',
  unavailable: 'Lifecycle not confirmed: continuity store unreachable',
  unenrolled: 'Lifecycle not confirmed: not enrolled',
  mismatch: 'Lifecycle held: it does not match the confirmed history',
  pending: 'Lifecycle change not confirmed yet',
  resurrection: 'Lifecycle held: not the attested identity instance',
}

/** What each readiness condition is called. */
const CONDITION: Record<ClientLifecycle['unmet'][number]['condition'], string> = {
  provisioning: 'Provisioning',
  identity: 'Identity',
  publication: 'Publication',
}

/**
 * Whether a client is saved intent or a usable client, and why.
 *
 * A saved client is not a working one: this is what keeps the console from
 * presenting a document an operator just wrote as a client its users can
 * reach. `compact` renders the pill alone, for a table row.
 */
export function LifecycleStatus({
  lifecycle,
  compact = false,
}: {
  lifecycle: ClientLifecycle
  compact?: boolean
}) {
  const pill = <Status value={lifecycle.state}>{LABEL[lifecycle.state]}</Status>
  if (compact) {
    return pill
  }

  return (
    <div className="lifecycle">
      {pill}
      {lifecycle.unmet.length > 0 && (
        <ul className="lifecycle__unmet" aria-label="Not ready yet">
          {lifecycle.unmet.map((unmet) => (
            <li key={unmet.condition}>
              <strong>{CONDITION[unmet.condition]}</strong>: {unmet.detail}
            </li>
          ))}
        </ul>
      )}
      {lifecycle.continuity.state === 'held' && (
        <p className="lifecycle__note">
          <strong>{HELD[lifecycle.continuity.reason]}</strong>: {lifecycle.continuity.detail}.
        </p>
      )}
      {lifecycle.tombstone !== null && (
        <p className="lifecycle__note">
          Retirement recorded {when(lifecycle.tombstone.at)} by {lifecycle.tombstone.by}:{' '}
          {lifecycle.tombstone.reason}. Its state is retained.
        </p>
      )}
      {lifecycle.withdrawal !== null && (
        <p className="lifecycle__note">
          <strong>{WITHDRAWAL[lifecycle.withdrawal.state]}</strong>: {lifecycle.withdrawal.detail}.
        </p>
      )}
      {lifecycle.restorations.map((restoration) => (
        <p className="lifecycle__note" key={`${String(restoration.tombstone.at)}-${String(restoration.at)}`}>
          Restored {when(restoration.at)} by {restoration.by}: {restoration.reason}. Retirement recorded{' '}
          {when(restoration.tombstone.at)} by {restoration.tombstone.by} ({restoration.tombstone.reason}) is kept
          in its history.
        </p>
      ))}
      {lifecycle.deletionConfirmed !== null && (
        <p className="lifecycle__note">
          Deletion confirmed {when(lifecycle.deletionConfirmed.at)} by {lifecycle.deletionConfirmed.by}. This
          records the decision only; nothing has been deleted.
        </p>
      )}
    </div>
  )
}

function when(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString()
}
