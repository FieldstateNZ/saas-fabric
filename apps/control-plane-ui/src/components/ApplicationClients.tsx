import type { ApplicationClient } from '../api/types'

/**
 * The applications a client's realm holds.
 *
 * Read-only in this increment, and shown rather than hidden: an operator
 * looking at a client's identity needs to know which applications can sign its
 * users in, even before the console can change them.
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
