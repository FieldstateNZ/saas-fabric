/**
 * The shapes the control-plane API speaks about runtime publication
 * (ADR 0023 part 4).
 *
 * Split from `./types` the way `./data-source-types` already is: this
 * describes what the runtime-publication target holds and what the last
 * pass did, a different conversation from a component's desired and running
 * versions.
 */

/** What a publication pass finished as. */
export type PassOutcome = 'published' | 'unchanged' | 'waiting' | 'refused' | 'failed'

/**
 * The revision currently held for each of the runtime's three documents.
 *
 * Every field is `null` before a first pass has run, and after a pass whose
 * outcome was `waiting`, `refused` or `failed` — a field reports a revision
 * only for a pass that published or found the target unchanged, so a number
 * here always means those bytes are what the target holds right now. Never
 * `0` for "nothing published": the platform is silent about what the target
 * holds, not reporting an empty one.
 */
export interface PublishedDocuments {
  readonly tenants: number | null
  readonly dataSources: number | null
  readonly catalog: number | null
  /** The issuer registry's generation, carried with the tenants document. */
  readonly registry: number | null
}

/**
 * One readiness gate (D01-18). `met` is Fabric's own evidence only: the
 * platform still decides what the edge and the runtime applied, and an
 * operator still raises replicas deliberately.
 */
export interface Gate {
  readonly met: boolean
  readonly reasons: readonly string[]
}

/**
 * Why a bound tenant is left out of the generated issuer registry. `notReady`
 * includes identity evidence pending reconciliation, which is temporary;
 * `retired` is a recorded retirement.
 */
export type ExclusionKind = 'notReady' | 'retired' | 'noClient' | 'invalidIssuer'

/**
 * A bound tenant left out of the issuer registry Fabric generated. This is
 * exclusion from a published registry, not route withdrawal: no runtime reads
 * the registry and edge application is not observed — see
 * `RegistryApplication`.
 */
export interface ExcludedTenant {
  readonly tenant: string
  readonly kind: ExclusionKind
  readonly detail: string
}

/**
 * What Fabric knows about the registry being applied: always `unobserved`.
 * A tenant left out of it may still be served by trust configured elsewhere.
 */
export interface RegistryApplication {
  readonly state: 'unobserved'
  readonly detail: string
}

/**
 * The three gates the runtime is raised against, named as the platform
 * contract names them. `connectors` is never met here: platform
 * composition deploys them, and Fabric does not observe it.
 */
export interface Readiness {
  readonly documents: Gate
  readonly identity: Gate
  readonly connectors: Gate
  readonly excludedFromRegistry: readonly ExcludedTenant[]
  readonly registryApplication: RegistryApplication
}

/**
 * What the last publication pass did.
 *
 * `detail` is a sanitised sentence for `waiting`, `refused` and `failed`, and
 * `null` for `published` and `unchanged` — there is nothing to explain about
 * a pass that did exactly what it was asked.
 */
export interface LastPass {
  readonly atUnixSeconds: number
  readonly outcome: PassOutcome
  readonly detail: string | null
}

/**
 * What this environment's runtime-publication target holds, and what the
 * last pass did.
 *
 * `lastPass` is `null` when no pass has run yet — distinct from a pass that
 * ran and found nothing to change, the same distinction `Platform.lastCheck`
 * draws for reconciliation. `Platform.publication` itself is `null` one level
 * up, for the narrower case: no target configured at all.
 */
export interface Publication {
  readonly target: string
  readonly documents: PublishedDocuments
  readonly lastPass: LastPass | null
  readonly readiness: Readiness
}
