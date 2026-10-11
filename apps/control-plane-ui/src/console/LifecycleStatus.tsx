import type { ClientLifecycle, LifecycleState } from '../api/lifecycle-types'
import { Status } from './Status'

/** The words an operator sees for each state. */
const LABEL: Record<LifecycleState, string> = {
  incomplete: 'Saved, not usable yet',
  ready: 'Ready',
  tombstoned: 'Tombstoned',
  deletionConfirmed: 'Deletion confirmed',
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
      {lifecycle.tombstone !== null && (
        <p className="lifecycle__note">
          Tombstoned {when(lifecycle.tombstone.at)} by {lifecycle.tombstone.by}: {lifecycle.tombstone.reason}.
          Its state is retained and it is no longer published.
        </p>
      )}
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
