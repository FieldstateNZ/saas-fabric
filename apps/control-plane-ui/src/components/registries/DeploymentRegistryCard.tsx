import type { DeploymentRegistry, Registry } from '../../api/registry-types'

/**
 * The deployment's own registry: where this deployment reads its own
 * components, set by its configuration rather than registered here.
 *
 * # Listed as the deployment's, and anonymous
 *
 * No operator registered it, so nothing on this card can be changed or
 * removed from the console, and it holds no credential of its own. An
 * operator may register the same host, at the same endpoint, to add a
 * credential and repositories to it; that registration is its own card,
 * and removing it restores this one's anonymous reading.
 */
export function DeploymentRegistryCard({
  deployment,
  registered,
}: {
  readonly deployment: DeploymentRegistry
  /** An operator's registration for the same host, if there is one. */
  readonly registered: Registry | undefined
}) {
  return (
    <section className="integration registry" aria-labelledby="registry-deployment">
      <h3 className="integration__name" id="registry-deployment">
        The deployment’s registry
      </h3>

      <dl className="integration__rows">
        <dt>Host</dt>
        <dd>{deployment.host}</dd>
        <dt>Endpoint</dt>
        <dd className="mono">{deployment.endpoint}</dd>
        <dt>Set by</dt>
        <dd>This deployment’s configuration</dd>
        <dt>Credential</dt>
        <dd>None of its own. Read anonymously.</dd>
      </dl>

      {registered !== undefined &&
        (registered.installed ? (
          <p className="integration__detail">
            {deployment.host} is also registered below. A credential held there is presented only
            for the repositories registered there; every other repository is read anonymously.
          </p>
        ) : (
          <p className="integration__detail">
            {deployment.host} is also registered below, and nothing is read through that
            registration: everything here is read anonymously.
          </p>
        ))}
    </section>
  )
}
