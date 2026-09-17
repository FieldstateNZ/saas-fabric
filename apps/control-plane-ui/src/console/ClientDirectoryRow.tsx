import type { Client } from '../api/types'
import { clientHref } from './navigation'
import { Status } from './Status'
import type { IdentityEntry } from './useInventory'

/**
 * One row of `ClientDirectory`'s table.
 *
 * `entry` is `undefined` until the shared identity inventory has read this
 * client, which is a different situation from `loading` being false with no
 * identity at all (a read that failed) — but the status column does not
 * currently tell them apart: both render "Unavailable" once loading is
 * false. Only the loading state itself ("Loading") is distinguished.
 */
export function ClientDirectoryRow({
  client,
  entry,
  loading,
}: {
  client: Client
  entry: IdentityEntry | undefined
  loading: boolean
}) {
  return (
    <tr>
      <td>
        <a className="entity" href={clientHref(client.id)}>
          <span className="entity-avatar">{client.displayName.charAt(0)}</span>
          <span>
            <strong>{client.displayName}</strong>
            <small>{client.id}</small>
          </span>
        </a>
      </td>
      <td className="mono">{client.hosts.join(', ') || 'No domains'}</td>
      <td className="mono">{client.realm}</td>
      <td>
        <Status value={loading ? 'unknown' : (entry?.identity?.reconciliation.status ?? 'unknown')}>
          {loading ? 'Loading' : (entry?.identity?.reconciliation.status ?? 'Unavailable')}
        </Status>
      </td>
      <td>
        <a href={clientHref(client.id)} aria-label={`Open ${client.displayName}`}>
          ↗
        </a>
      </td>
    </tr>
  )
}
