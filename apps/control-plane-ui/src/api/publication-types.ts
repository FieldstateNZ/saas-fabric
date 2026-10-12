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
 * What the runtime's replicas were found serving (G7). Only `applied` is a
 * positive claim, and only about the sample taken at `observedAtUnixSeconds`:
 * every replica that could serve reported the stamped record. `noReaders` is
 * replicas at zero: never applied, and not evidence of withdrawal.
 */
export type ReaderState = 'applied' | 'pending' | 'unobserved' | 'noReaders'

/** What one replica's answer means against the stamped record. */
export type ReaderFinding =
  | 'matches'
  | 'behind'
  | 'lastKnownGood'
  | 'ahead'
  | 'wrongDigest'
  | 'nothingServed'
  | 'failed'
  | 'notReady'
  | 'terminating'
  | 'uncompared'

/** One publication record, as a replica identifies it. */
export interface RecordStamp {
  readonly generation: number
  readonly recordSha256: string
}

/** A generation one replica reported serving, and where it read it from. */
export interface ServedGeneration extends RecordStamp {
  readonly origin: 'published' | 'lastKnownGood'
}

/** One pod the runtime Deployment owns, through any of its replica sets. */
export interface ReaderMember {
  readonly pod: string
  readonly replicaSet: string
  readonly finding: ReaderFinding
  /** Why it gave no usable answer; `null` unless `finding` is `failed`. */
  readonly detail: string | null
  readonly served: ServedGeneration | null
}

/** The readers, sampled and held to the record the last accepted pass stamped. */
export interface Readers {
  readonly state: ReaderState
  readonly reasons: readonly string[]
  readonly expected: RecordStamp | null
  readonly observedAtUnixSeconds: number | null
  readonly desiredReplicas: number | null
  readonly members: readonly ReaderMember[]
}

/**
 * The three gates the runtime is raised against, named as the platform
 * contract names them. `connectors` is never met here: platform
 * composition deploys them, and Fabric does not observe it. `readers` sits
 * beside them: the documents gate stays publisher evidence, so a runtime at
 * zero replicas can still be raised, and `updateComplete` is true only once
 * the documents are published and the readers applied them.
 */
export interface Readiness {
  readonly documents: Gate
  readonly identity: Gate
  readonly connectors: Gate
  readonly excludedFromRegistry: readonly ExcludedTenant[]
  readonly registryApplication: RegistryApplication
  readonly readers: Readers
  readonly updateComplete: boolean
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
 * Whether the target held one whole recorded generation when a pass last
 * inspected it (G3): every document, and the issuer registry the record
 * names, at the record's revision and digest. Kept apart from `LastPass`: a
 * pass can fail having written nothing and leave the target complete. Only
 * `complete` can meet the documents gate. It is about the target, not the
 * readers, which keep their last complete generation either way.
 * `unobserved` with `observedAtUnixSeconds === null` means no pass has
 * inspected the target since the control plane started.
 */
export type CompletenessState = 'complete' | 'incomplete' | 'unpublished' | 'unobserved'

export interface Completeness {
  readonly state: CompletenessState
  readonly observedAtUnixSeconds: number | null
  /** The record inspected against, for `complete` and `incomplete`. */
  readonly record: RecordStamp | null
  /** Why it is not `complete`; names parts, never their contents. */
  readonly reason: string | null
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
  readonly completeness: Completeness
  readonly readiness: Readiness
}
