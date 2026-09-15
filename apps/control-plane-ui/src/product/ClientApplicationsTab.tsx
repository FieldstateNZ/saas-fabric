import type { ClientProductResponse } from '../api/catalogue-types'
import { Panel } from '../console/Panel'
import { Status } from '../console/Status'

/**
 * The Applications tab of `ClientWorkspace`: every application this client
 * is assigned, at the exact release version it was assigned.
 *
 * `assignment.release` is that resolved release, not the application's
 * current draft — a published release is immutable, so the plan name,
 * fields, and component list shown here are what the client was actually
 * assigned, even if the application has since been edited further.
 */
export function ClientApplicationsTab({ data }: { data: ClientProductResponse }) {
  return (
    <>
      {data.product.applications.length === 0 && (
        <p>No applications assigned. Configure the client to add a published application.</p>
      )}
      {data.product.applications.map((assignment) => (
        <Panel
          key={assignment.applicationId}
          title={assignment.release.definition.name}
          action={<Status value="neutral">Definition v{assignment.release.version}</Status>}
        >
          <div className="panel-body">
            <p>
              {assignment.release.definition.plans.find((p) => p.id === assignment.planId)?.name}{' '}
              plan
            </p>
            <p>
              Components:{' '}
              {data.resolved
                .find((r) => r.applicationId === assignment.applicationId)
                ?.components.map((c) => c.name)
                .join(', ') || 'None'}
            </p>
            <dl className="facts">
              {Object.entries(assignment.configuration).map(([key, value]) => (
                <div className="fact-pair" key={key}>
                  <dt>
                    {assignment.release.definition.fields.find((f) => f.key === key)?.label ?? key}
                  </dt>
                  <dd>{value}</dd>
                </div>
              ))}
            </dl>
            <a href={`#/applications/${encodeURIComponent(assignment.applicationId)}`}>
              Open application definition →
            </a>
          </div>
        </Panel>
      ))}
    </>
  )
}
