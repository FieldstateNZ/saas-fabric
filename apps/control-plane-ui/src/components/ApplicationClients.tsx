import type { ApplicationClient } from '../api/types'

/**
 * The applications a client's realm holds.
 *
 * Read-only in this increment, and shown rather than hidden: an operator
 * looking at a client's identity needs to know which applications can sign its
 * users in, even before the console can change them.
 *
 * # Only what Fabric observed
 *
 * The strategy and the PKCE method are rendered exactly as the API sent them
 * -- including a strategy this build does not otherwise recognise -- rather
 * than mapped through a lookup table that would have to guess at an
 * unfamiliar value or drop it. The badge reuses `ReconciliationBadge`'s
 * `badge`/`badge__status` classes rather than inventing a second way to mark
 * up an enum-like value.
 */
export function ApplicationClients({ clients }: { clients: readonly ApplicationClient[] }) {
  if (clients.length === 0) {
    return <p className="empty">No applications are declared for this client yet.</p>
  }

  return (
    <ul className="applications">
      {clients.map((application) => (
        <li key={application.id}>
          <span className="applications__id">{application.id}</span>
          <div className={`badge badge--${application.redirect.strategy}`}>
            <span className="badge__status">{application.redirect.strategy}</span>
          </div>
          <p className="identity__note">PKCE {application.pkce.toUpperCase()}</p>
          <ul className="applications__uris">
            {application.redirect.uris.map((uri) => (
              <li key={uri}>{uri}</li>
            ))}
          </ul>
        </li>
      ))}
    </ul>
  )
}
