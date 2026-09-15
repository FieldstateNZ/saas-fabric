import { EmptyState } from './EmptyState'
import { clientHref } from './navigation'
import { Panel } from './Panel'
import { Status } from './Status'
import type { Inventory } from './useInventory'

/**
 * The most recent identity observations, across every client.
 *
 * Shared by the dashboard (a short list) and the reconciliation page (the
 * full one) via `limit`. The API returns current observations, not an audit
 * history — this is "what reconciliation last found for each client", not a
 * log of every pass that has ever run.
 */
export function Activity({ inventory, limit }: { inventory: Inventory; limit?: number }) {
  const sorted = [...inventory.entries].sort(
    (a, b) =>
      (b.identity?.reconciliation.observedAtUnix ?? 0) -
      (a.identity?.reconciliation.observedAtUnix ?? 0),
  )

  return (
    <Panel title="Latest observations" action={<a href="#/reconciliation">Reconciliation →</a>}>
      {inventory.loading ? (
        <p className="panel-body" role="status">
          Loading observations…
        </p>
      ) : sorted.length === 0 ? (
        <EmptyState title="No observations yet">
          <p>Connect client configuration to see reconciliation results.</p>
        </EmptyState>
      ) : (
        <div className="table-wrap">
          <table className="fabric-table">
            <thead>
              <tr>
                <th>Client</th>
                <th>Observation</th>
                <th>Status</th>
                <th>Last checked</th>
              </tr>
            </thead>
            <tbody>
              {sorted.slice(0, limit).map(({ client, identity, error }) => (
                <tr key={client.id}>
                  <td>
                    <a href={clientHref(client.id)}>{client.displayName}</a>
                  </td>
                  <td>{error ?? identity?.reconciliation.detail ?? 'Identity configuration'}</td>
                  <td>
                    <Status value={identity?.reconciliation.status ?? 'unknown'} />
                  </td>
                  <td className="mono">
                    {identity?.reconciliation.observedAtUnix == null
                      ? 'Not observed'
                      : new Date(identity.reconciliation.observedAtUnix * 1000).toLocaleString()}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Panel>
  )
}
