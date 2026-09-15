import type { ClientProductResponse } from '../api/catalogue-types'
import { Panel } from '../console/Panel'
import { Status } from '../console/Status'

/**
 * The Health tab of `ClientWorkspace`: identity reconciliation, which this
 * console does observe, and every assigned component, which it does not.
 *
 * Every component reads "Deployment not observed" rather than guessing a
 * status from desired state — there is no deployment controller behind
 * these components yet (see PHASE_ONE.md), and reporting anything else would
 * be reporting a rollout that never happened.
 */
export function ClientHealthTab({ data }: { data: ClientProductResponse }) {
  return (
    <Panel title="Reconciliation status">
      <div className="panel-body">
        <p>
          Identity <Status value={data.reconciliation.status} />
        </p>
        <p>{data.reconciliation.detail}</p>
        {data.resolved
          .flatMap((r) => r.components)
          .map((component, index) => (
            <p key={`${component.id}-${String(index)}`}>
              {component.name} <Status value="neutral">Deployment not observed</Status>
            </p>
          ))}
        <a href="#/reconciliation">Check reconciliation →</a>
      </div>
    </Panel>
  )
}
