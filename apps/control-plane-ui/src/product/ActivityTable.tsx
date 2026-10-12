import type { ProductActivity } from '../api/catalogue-types'
import { Panel } from '../console/Panel'

/**
 * A client's or the platform's recent activity, most recent first.
 *
 * Shared by `ClientWorkspace`'s Activity tab and {@link ProductActivityFeed}:
 * both have a list of {@link ProductActivity} and nothing else that differs
 * about how it should be shown.
 *
 * It says what it is: a view of recent changes kept for convenience, not the
 * audit trail (ADR 0028). `omitted`, when the API bounded the view, says how
 * many older actions are not shown, so a bounded list never reads as the
 * whole record.
 */
export function ActivityTable({
  activity,
  omitted = 0,
}: {
  activity: readonly ProductActivity[]
  omitted?: number
}) {
  return (
    <Panel title="Recent activity">
      <p className="panel-body">
        A view of recent changes, not the audit trail.
        {omitted > 0 && ` ${String(omitted)} older ${omitted === 1 ? 'action is' : 'actions are'} not shown.`}
      </p>
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
