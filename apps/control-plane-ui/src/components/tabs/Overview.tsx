import type { Client } from '../../api/types'

/**
 * What SaaS Fabric knows about one client, and what it does not.
 *
 * # Holes are rendered, not omitted
 *
 * Every field the product intends to show is listed. The ones with no API
 * behind them say so in place, because a screen that silently drops what it
 * cannot show is a screen that lies about how much of the platform is wired
 * up — and this console's first job is to make that legible.
 */
export function Overview({ client }: { client: Client }) {
  return (
    <section className="overview">
      <dl className="overview__fields">
        <dt>Client</dt>
        <dd>
          {client.displayName} <span className="overview__muted">({client.id})</span>
        </dd>

        <dt>Realm</dt>
        <dd>{client.realm}</dd>

        <dt>Domains</dt>
        <dd>{client.hosts.length === 0 ? 'None yet' : client.hosts.join(', ')}</dd>

        {MISSING.map(([field, why]) => (
          <Missing key={field} field={field} why={why} />
        ))}
      </dl>
    </section>
  )
}

/** One field the product intends to show and cannot yet. */
function Missing({ field, why }: { field: string; why: string }) {
  return (
    <>
      <dt>{field}</dt>
      <dd className="overview__missing">
        Not exposed yet <span className="overview__muted">— {why}</span>
      </dd>
    </>
  )
}

/**
 * The Overview's holes, each naming what it would take to fill it.
 *
 * A list rather than seven pasted blocks, so that deleting an entry is the
 * whole of the work when its API arrives.
 */
const MISSING: readonly (readonly [string, string])[] = [
  ['Issuer', 'sign-in issuer details are not available'],
  ['Secret partition', 'secret partition details are not available'],
  ['Authorization store', 'authorization details are not available'],
  ['Authorization model', 'authorization model details are not available'],
  ['Database endpoint', 'database placement is not available'],
  ['Modules', 'module assignments are not available'],
  ['Provisioning health', 'check Identity for the latest reconciliation result'],
]
