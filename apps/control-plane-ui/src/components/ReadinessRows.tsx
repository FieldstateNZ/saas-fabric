import type { Gate, Readiness } from '../api/publication-types'
import { Status } from '../console/Status'

/**
 * The three readiness gates (D01-18), and the published tenants no issuer
 * reaches. Read-only, like `PublicationRows`: a gate shown as met is
 * Fabric's evidence, not a claim that the runtime may be raised.
 */
export function ReadinessRows({ readiness }: { readiness: Readiness }) {
  return (
    <dl className="facts">
      <dt>Documents gate</dt>
      <dd>
        <GateCell gate={readiness.documents} />
      </dd>

      <dt>Identity gate</dt>
      <dd>
        <GateCell gate={readiness.identity} />
      </dd>

      <dt>Connectors gate</dt>
      <dd>
        <GateCell gate={readiness.connectors} />
      </dd>

      <dt>Not routed</dt>
      <dd>
        {readiness.withheld.length === 0
          ? 'None'
          : readiness.withheld.map((entry) => `${entry.tenant}: ${entry.reason}`).join('; ')}
      </dd>
    </dl>
  )
}

function GateCell({ gate }: { gate: Gate }) {
  return gate.met ? (
    <Status value="applied">Met</Status>
  ) : (
    <>
      <Status value="pending">Unmet</Status> — {gate.reasons.join('; ')}
    </>
  )
}
