import { V1_API_VERSION, type ApplicationClient, type RedirectStrategyName } from '../api/types'

/** Every redirect strategy this build recognises. */
const KNOWN_STRATEGIES: readonly RedirectStrategyName[] = [
  'claimedHttps',
  'privateNetwork',
  'development',
  'customScheme',
]

/**
 * The `badge--<strategy>` modifier class for a redirect strategy, or none.
 *
 * The strategy is a wire value this build did not necessarily mint -- a
 * future strategy must still render (see the module docs) -- so it is
 * whitelisted before it reaches a class list rather than interpolated
 * unexamined. An unrecognised value falls back to no modifier; the base
 * `.badge` styling still applies, and the value itself is still shown as
 * text below, untouched.
 */
function strategyModifier(strategy: RedirectStrategyName): string {
  return KNOWN_STRATEGIES.includes(strategy) ? `badge--${strategy}` : ''
}

/**
 * The applications a client's realm holds, and the document they are declared
 * in.
 *
 * Read-only in this increment, and shown rather than hidden: an operator
 * looking at a client's identity needs to know which applications can sign its
 * users in, even before the console can change them.
 *
 * # Only what Fabric observed
 *
 * The redirect strategy and the schema version are rendered exactly as the
 * API sent them -- including a strategy this build does not otherwise
 * recognise -- rather than mapped through a lookup table that would have to
 * guess at an unfamiliar value or drop it. Both use `badge__status--literal`
 * so CSS cannot silently change what "exactly" means: `.badge__status`'s
 * default `text-transform: capitalize` would otherwise display `claimedHttps`
 * as `ClaimedHttps`, a spelling the API refuses.
 *
 * The PKCE method is not held to that same "exactly as sent" rule: it is
 * uppercased for the label next to it (`s256` reads as `PKCE S256`), a
 * presentation choice, not a guess -- a method this build does not recognise
 * is still shown in full, just not in the case the API sent it.
 *
 * The strategy and version badges reuse `ReconciliationBadge`'s
 * `badge`/`badge__status`/`badge__explanation` classes rather than inventing
 * a second way to mark up an enum-like value.
 */
export function ApplicationClients({
  clients,
  apiVersion,
}: {
  clients: readonly ApplicationClient[]
  apiVersion: string
}) {
  return (
    <>
      <div className="badge">
        <span className="badge__status badge__status--literal">{apiVersion}</span>
        <p className="badge__explanation">document version</p>
        {apiVersion === V1_API_VERSION && (
          <p className="badge__explanation">An edit will migrate this document to v2.</p>
        )}
      </div>

      {clients.length === 0 ? (
        <p className="empty">No applications are declared for this client yet.</p>
      ) : (
        <ul className="applications">
          {clients.map((application) => (
            <li key={application.id}>
              <span className="applications__id">{application.id}</span>
              <div className={`badge ${strategyModifier(application.redirect.strategy)}`.trim()}>
                <span className="badge__status badge__status--literal">{application.redirect.strategy}</span>
                <p className="badge__explanation">redirect strategy</p>
              </div>
              <p className="identity__note">PKCE {application.pkce.toUpperCase()}</p>
              <ul className="applications__uris">
                {application.redirect.uris.map((uri) => (
                  <li key={uri}>{uri}</li>
                ))}
              </ul>
            </li>
          ))}
        </ul>
      )}
    </>
  )
}
