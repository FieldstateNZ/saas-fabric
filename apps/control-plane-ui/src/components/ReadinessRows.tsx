import type { ExcludedTenant, ExclusionKind, Gate, Readiness } from '../api/publication-types'
import { Status } from '../console/Status'

/**
 * The three readiness gates (D01-18), the bound tenants the generated issuer
 * registry leaves out, and what is known about the registry being applied.
 *
 * Read-only, like `PublicationRows`. A gate shown as met is Fabric's
 * evidence, not a claim that the runtime may be raised; a tenant left out of
 * the registry is not shown as unrouted, because nothing here observes an
 * edge or a runtime applying the registry.
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

      <dt>Excluded from registry</dt>
      <dd>
        {readiness.excludedFromRegistry.length === 0
          ? 'None'
          : readiness.excludedFromRegistry.map(excluded).join('; ')}
      </dd>

      <dt>Registry applied</dt>
      <dd>
        <Status value="pending">Unobserved</Status> — {readiness.registryApplication.detail}
      </dd>
    </dl>
  )
}

const KIND: Record<ExclusionKind, string> = {
  notReady: 'not ready',
  retired: 'retired',
  noClient: 'no client',
  invalidIssuer: 'invalid issuer',
}

function excluded(entry: ExcludedTenant): string {
  return `${entry.tenant} (${KIND[entry.kind]}): ${entry.detail}`
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
