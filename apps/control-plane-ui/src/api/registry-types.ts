/**
 * The shapes the control-plane API speaks about image registries (ADR 0026
 * section 5).
 *
 * Its own file, as `./placement-types` is: registries are an integration of
 * their own, not a third Git application, and nothing here is shared with
 * the Git integrations' types.
 *
 * # What is never here
 *
 * A token. The API never returns one -- there is no reveal -- so there is
 * no field that could hold one, and a component rendering these types has
 * nothing secret to render by mistake.
 */

/**
 * The closed set of registry kinds.
 *
 * `ghcr` and `dockerHub` fix their own host and endpoint; only
 * `distribution` is served where an operator says.
 */
export type RegistryKind = 'ghcr' | 'dockerHub' | 'distribution'

/**
 * Who holds a registry's credential, and what became of it. Never the token.
 *
 * `refused` and `unreadable` are what the control plane observed, not a
 * guess: the realm refused it, so it is not presented again until it is
 * replaced; or it could not be read at the last start, so the registry is
 * read anonymously until it is set again.
 */
export interface RegistryCredential {
  readonly username: string
  readonly setBy: string
  /** Unix seconds. */
  readonly setAt: number
  readonly refused: boolean
  readonly unreadable: boolean
}

/** A repository registered under a registry. */
export interface RegisteredRepository {
  /** Its full name, starting with the registry's host. */
  readonly repository: string
  /** When its tag listing last answered, in Unix seconds. */
  readonly provenAt: number
}

/**
 * One registry an operator registered: what was proven, never "connected".
 *
 * `realmOrigin` is the token realm its `/v2/` endpoint named when it was
 * proven, or `null` when it named none. `deployment` says this is the
 * deployment's host, contributing a credential and repositories to the
 * deployment's registry. `installed` is `false` only when a restart could
 * not rebuild it.
 */
export interface Registry {
  readonly host: string
  readonly kind: RegistryKind
  readonly endpoint: string
  readonly realmOrigin: string | null
  readonly deployment: boolean
  readonly installed: boolean
  readonly credential: RegistryCredential | null
  readonly registeredBy: string
  /** Unix seconds. */
  readonly registeredAt: number
  readonly repositories: readonly RegisteredRepository[]
}

/** The deployment's own registry: configuration, not a registration. */
export interface DeploymentRegistry {
  readonly host: string
  readonly endpoint: string
}

/**
 * Every registry operators registered, and the deployment's own.
 *
 * `deployment` is `null` when this deployment manages no platform.
 */
export interface RegistryListing {
  readonly registries: readonly Registry[]
  readonly deployment: DeploymentRegistry | null
}

/**
 * What an operator registers.
 *
 * No host: `ghcr` and `dockerHub` are named by their kind, and a
 * `distribution` registry by its endpoint's. `endpoint` is sent only for
 * `distribution`, and `username` and `token` together or not at all -- the
 * control plane refuses anything else, and says which rule.
 */
export interface RegistryRegistration {
  readonly kind: RegistryKind
  readonly endpoint?: string
  readonly username?: string
  readonly token?: string
}

/** A credential being set. The token goes one way: into the request body. */
export interface RegistryCredentialInput {
  readonly username: string
  readonly token: string
}

/** A registered repository's version tags, for the picker. */
export interface RegistryVersions {
  /** Tags that are component versions, newest first. */
  readonly tags: readonly string[]
  /** How many tags are not versions. */
  readonly other: number
}
