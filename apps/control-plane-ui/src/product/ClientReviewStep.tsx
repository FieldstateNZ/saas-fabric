import type { ClientProductRequest } from '../api/catalogue-types'

/**
 * The last step of {@link ClientForm}: a summary of what is about to be
 * saved, and what saving does and does not do.
 *
 * The closing note is the reason this step exists rather than a plain submit
 * button on step two: saving only writes desired state. Identity
 * reconciliation follows separately and on its own schedule, and neither
 * deployment nor routing for any assigned application is observed by this
 * console at all — an operator reviewing "what is about to happen" needs
 * that stated plainly, not implied by a spinner that eventually stops.
 */
export function ClientReviewStep({ id, value }: { id: string; value: ClientProductRequest }) {
  return (
    <section className="panel panel-body">
      <h2>{value.displayName}</h2>
      <dl className="facts">
        <dt>Client ID</dt>
        <dd>{id}</dd>
        <dt>Legal name</dt>
        <dd>{value.legalName}</dd>
        <dt>Region</dt>
        <dd>{value.region}</dd>
        <dt>Timezone</dt>
        <dd>{value.timezone}</dd>
        <dt>Hostnames</dt>
        <dd>{value.hosts.join(', ') || 'None'}</dd>
      </dl>
      <h3>Application assignments</h3>
      {value.applications.length === 0 ? (
        <p>No applications selected.</p>
      ) : (
        value.applications.map((assignment) => (
          <p key={assignment.applicationId}>
            {assignment.applicationId} · {assignment.planId} · definition v{assignment.version}
          </p>
        ))
      )}
      <p>
        Saving writes desired state. Identity reconciliation follows separately; deployment and
        routing are not observed.
      </p>
    </section>
  )
}
