import type { HostReads, Registry } from '../../api/registry-types'
import type { RegistriesState } from '../../hooks/useRegistries'
import { ComponentReadsPart } from './ComponentReads'
import { KIND_NAMES, when } from './registry-words'
import { RegistryCredentialPart } from './RegistryCredentialPart'
import { RegistryRepositories } from './RegistryRepositories'
import { RemoveRegistry } from './RemoveRegistry'

/**
 * One registry an operator registered, as the control plane reports it.
 *
 * # What was proven, never "connected"
 *
 * Its kind, host and endpoint are what it was registered with and cannot
 * change. The token realm is the origin its `/v2/` endpoint named when it
 * was proven: for a `distribution` registry, recorded then and held to
 * since, and for a hosted kind always its kind's own; `None` means it named
 * none.
 * Nothing here claims the registry answers now -- nothing has asked it
 * since each thing on this card was proven, and each says when that was.
 *
 * The one live fact is `installed`, and it is shown only when it is false:
 * the control plane could not rebuild this registry when it last started,
 * so nothing is being read through it -- and nothing on it can change but
 * withdrawing its credential or removing it, because any other change is
 * proven through a client the control plane will not build.
 */
export function RegistryCard({
  registry,
  registries,
  reads,
}: {
  readonly registry: Registry
  readonly registries: RegistriesState
  /** The managed components' images read through this registry's host, when observed. */
  readonly reads: HostReads | undefined
}) {
  const headingId = `registry-${registry.host}`

  return (
    <section className="integration registry" aria-labelledby={headingId}>
      <h3 className="integration__name" id={headingId}>
        {registry.host}
      </h3>

      <dl className="integration__rows">
        <dt>Kind</dt>
        <dd>{KIND_NAMES[registry.kind]}</dd>
        <dt>Endpoint</dt>
        <dd className="mono">{registry.endpoint}</dd>
        <dt>Token realm</dt>
        <dd className={registry.realmOrigin === null ? undefined : 'mono'}>
          {registry.realmOrigin ?? 'None named when it was proven'}
        </dd>
        <dt>Registered</dt>
        <dd>
          by {registry.registeredBy}, {when(registry.registeredAt)}
        </dd>
      </dl>

      {registry.deployment && (
        <p className="integration__detail">
          The deployment’s host. This registration adds a credential and repositories to the
          deployment’s registry, at the deployment’s endpoint.
        </p>
      )}

      {!registry.installed && (
        <p className="integration__diagnostic">
          Nothing is read through this registry: the control plane could not rebuild it when it
          last started. Its log says why.
        </p>
      )}

      <RegistryCredentialPart
        registry={registry}
        registries={registries}
        settable={registry.installed}
      />
      <RegistryRepositories
        registry={registry}
        registries={registries}
        changeable={registry.installed}
      />
      <ComponentReadsPart host={reads} />
      <RemoveRegistry
        host={registry.host}
        held={registry.credential !== null}
        registries={registries}
      />
    </section>
  )
}
