import type { UnboundCheck } from '../api/placement-types'
import { Panel } from '../console/Panel'

/**
 * The runtime catalogue's resources this client cannot reach (G8, ADR 0023
 * "A resource on a logical data source a client never asked for").
 *
 * The runtime serves one catalogue to every tenant. A resource on a logical
 * data source this client's tenant has no placement for answers
 * `UnboundDataSource` (500) to it, and the publisher still publishes that
 * catalogue, because another tenant may bind it (ADR 0018 part 4). So this
 * panel is the one place an operator sees those pairs.
 *
 * Checked against the derived runtime catalogue and the recorded
 * placements: what the next publication offers, not what a running replica
 * serves.
 */
export function UnboundResources({ unbound }: { unbound: UnboundCheck }) {
  if (unbound.state === 'unchecked') {
    return (
      <Panel title="Unreachable resources">
        <p className="panel-body error" role="alert">
          Could not check this client against the runtime catalogue: {unbound.reason}
        </p>
      </Panel>
    )
  }

  if (unbound.dataSources.length === 0) {
    return (
      <Panel title="Unreachable resources">
        <p className="panel-body">
          Every resource in the runtime catalogue is on a logical data source this client has
          placed.
        </p>
      </Panel>
    )
  }

  return (
    <Panel title="Unreachable resources">
      <p className="panel-body">
        This client has no placement for these logical data sources, so a request for any
        resource on them fails with <span className="mono">UnboundDataSource</span>.
      </p>
      <div className="table-wrap">
        <table className="fabric-table">
          <thead>
            <tr>
              <th>Logical</th>
              <th>Resources</th>
              <th>To bind it</th>
            </tr>
          </thead>
          <tbody>
            {unbound.dataSources.map((entry) => (
              <tr key={entry.logical}>
                <td className="mono">{entry.logical}</td>
                <td>
                  {entry.resources.map((resource) => (
                    <div key={resource.resource}>
                      <span className="mono">{resource.resource}</span>
                      {` · ${resource.application} v${String(resource.version)}`}
                    </div>
                  ))}
                </td>
                <td>
                  {entry.declared
                    ? 'Place it above.'
                    : 'Add it to this client’s data, then place it.'}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Panel>
  )
}
