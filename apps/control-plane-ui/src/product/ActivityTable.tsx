import type { ProductActivity } from '../api/catalogue-types'
import { Panel } from '../console/Panel'

/**
 * A client's or the platform's recorded activity, most recent first.
 *
 * Shared by `ClientWorkspace`'s Activity tab and {@link ProductActivityFeed}:
 * both have a list of {@link ProductActivity} and nothing else that differs
 * about how it should be shown.
 */
export function ActivityTable({ activity }: { activity: readonly ProductActivity[] }) {
  return (
    <Panel title="Activity">
      <div className="table-wrap">
        <table className="fabric-table">
          <thead>
            <tr>
              <th>When</th>
              <th>Resource</th>
              <th>Action</th>
              <th>Operator</th>
            </tr>
          </thead>
          <tbody>
            {[...activity]
              .sort((a, b) => b.at - a.at)
              .map((action, index) => (
                <tr key={index}>
                  <td className="mono">{new Date(action.at * 1000).toLocaleString()}</td>
                  <td>{action.resource}</td>
                  <td>{action.action}</td>
                  <td>{action.operator}</td>
                </tr>
              ))}
          </tbody>
        </table>
        {activity.length === 0 && <p className="panel-body">No recorded changes yet.</p>}
      </div>
    </Panel>
  )
}
