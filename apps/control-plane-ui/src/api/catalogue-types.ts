/**
 * The shapes the control-plane API speaks about the product catalogue.
 *
 * Where `./types` describes a client's identity, this describes what a
 * client is *entitled to*: the applications a platform offers, the plans and
 * configuration fields that shape them, and what a specific client has been
 * assigned. None of these names a container image, a Helm chart, or a
 * Kubernetes resource that is actually running — a definition publishes
 * desired state, and a deployment controller that turns it into a running
 * application, with deployment, routing and certificate health an operator
 * can observe, is not part of this increment (see PHASE_ONE.md).
 */

import type { Client, Reconciliation } from './types'

/** One field a client supplies as part of its own configuration. */
export interface ConfigurationField {
  readonly key: string
  readonly label: string
  readonly kind: 'text' | 'number' | 'boolean' | 'choice' | 'hostname' | 'identifier' | 'timezone'
  readonly required: boolean
  readonly default: string | null
  readonly options: readonly string[]
  readonly description: string
}

/** A set of configuration values, keyed by a field's `key`. */
export type Values = Readonly<Record<string, string>>

/**
 * One piece of an application definition: a container, a Helm chart, or a
 * capability the platform itself already provides.
 *
 * `kind: 'capability'` is never deployed — its `reference` names a platform
 * capability such as Identity or Secrets, and there is no image or chart
 * behind it for `version` or `policy` to describe.
 */
export interface ApplicationComponent {
  readonly id: string
  readonly name: string
  readonly kind: 'container' | 'helm' | 'capability'
  readonly reference: string
  readonly version: string
  readonly required: boolean
  readonly policy: 'automatic' | 'manual'
}

/** One capability an application definition offers, and what implements it. */
export interface ApplicationFeature {
  readonly id: string
  readonly name: string
  readonly description: string
  readonly implementedBy: readonly string[]
}

/** One plan a client can be assigned: a bundle of features and configuration limits. */
export interface ApplicationPlan {
  readonly id: string
  readonly name: string
  readonly description: string
  readonly features: readonly string[]
  readonly configuration: Values
}

/**
 * One entry point a client application exposes, gated by a feature and a
 * permission it must itself enforce.
 */
export interface NavigationItem {
  readonly label: string
  readonly route: string
  readonly feature: string | null
  readonly permission: string
}

/**
 * An application, as an operator defines it.
 *
 * This is the exact shape that gets copied, whole, into an
 * {@link ApplicationRelease} on publish. There is nothing client-specific
 * here — see {@link ApplicationAssignment} for what one client is actually
 * entitled to.
 */
export interface ApplicationDefinition {
  readonly name: string
  readonly description: string
  readonly domain: string
  readonly components: readonly ApplicationComponent[]
  readonly features: readonly ApplicationFeature[]
  readonly plans: readonly ApplicationPlan[]
  readonly fields: readonly ConfigurationField[]
  readonly navigation: readonly NavigationItem[]
}

/**
 * One published, numbered snapshot of an application definition.
 *
 * Releases are immutable: publishing again creates a new version rather than
 * changing this one, and a client already assigned to a release keeps that
 * exact version until an operator explicitly moves it. Editing the draft
 * after publication does not move anybody who is already assigned.
 */
export interface ApplicationRelease {
  readonly version: number
  readonly note: string
  readonly publishedAt: number
  readonly definition: ApplicationDefinition
}

/** An application as the catalogue holds it: an editable draft, and every release ever published from it. */
export interface ProductApplication {
  readonly id: string
  readonly draft: ApplicationDefinition
  readonly releases: readonly ApplicationRelease[]
}

/** Platform-wide display defaults, edited from Settings. */
export interface ConsoleSettings {
  readonly platformName: string
  readonly defaultRegion: string
  readonly timezone: string
}

/**
 * A link to another, independently authenticated operator console.
 *
 * Registering one does not provision or connect anything. It is a pointer an
 * operator can follow, recorded here because the platform has nowhere else to
 * keep it — not a deployment, and not something this console can verify.
 */
export interface EnvironmentRegistration {
  readonly id: string
  readonly name: string
  readonly consoleUrl: string
  readonly description: string
}

/** One recorded product or identity change, or reconciliation pass. */
export interface ProductActivity {
  readonly at: number
  readonly operator: string
  readonly action: string
  readonly resource: string
}

/**
 * The catalogue: every application, the shared client definition fields,
 * platform settings, registered environments, and recorded activity.
 *
 * `definitionVersion` tracks the shared client fields, not any one
 * application's version — a client's own copy of it is
 * {@link ClientProduct.definitionVersion}.
 */
export interface Catalogue {
  readonly applications: readonly ProductApplication[]
  readonly clientFields: readonly ConfigurationField[]
  readonly settings: ConsoleSettings
  readonly environments: readonly EnvironmentRegistration[]
  readonly activity: readonly ProductActivity[]
  readonly definitionVersion: number
}

/** The catalogue, and the revision a write must be conditioned on. */
export interface StoredCatalogue {
  readonly catalogue: Catalogue
  readonly revision: string | null
}

/**
 * Every change an operator can make to the catalogue, sent as one write.
 *
 * A closed union rather than a generic patch: each action is a decision the
 * control plane validates on its own terms — a definition save is not a
 * publish, and a publish is not a client-field change — so the request shape
 * says which one this is, rather than leaving it to be inferred from which
 * fields happen to be present.
 */
export type CatalogueCommand =
  | { readonly action: 'createApplication'; readonly id: string; readonly name: string }
  | {
      readonly action: 'saveApplication'
      readonly id: string
      readonly definition: ApplicationDefinition
    }
  | { readonly action: 'publishApplication'; readonly id: string; readonly note: string }
  | { readonly action: 'saveDefinition'; readonly fields: readonly ConfigurationField[] }
  | { readonly action: 'saveSettings'; readonly settings: ConsoleSettings }
  | { readonly action: 'saveEnvironment'; readonly environment: EnvironmentRegistration }

/** One application a client is being assigned to, or already is. */
export interface AssignmentRequest {
  readonly applicationId: string
  readonly version: number
  readonly planId: string
  readonly configuration: Values
}

/** One application a client is entitled to, resolved to the exact release it was assigned. */
export interface ApplicationAssignment {
  readonly applicationId: string
  readonly release: ApplicationRelease
  readonly planId: string
  readonly configuration: Values
}

/** A client's product configuration. Identity is a separate concern — see `./types`. */
export interface ClientProduct {
  readonly legalName: string
  readonly region: string
  readonly timezone: string
  readonly definitionVersion: number
  readonly configuration: Values
  readonly applications: readonly ApplicationAssignment[]
  readonly activity: readonly ProductActivity[]
}

/**
 * What an operator submits to create or reconfigure a client's product setup.
 *
 * A replacement, like {@link IdentityRequest} in `./types`: every field the
 * operator can see and edit, sent back whole. A successful write is
 * `pending` until reconciliation confirms identity has caught up — this
 * request has no way to say otherwise, and no field here can make it so.
 */
export interface ClientProductRequest {
  readonly displayName: string
  readonly hosts: readonly string[]
  readonly legalName: string
  readonly region: string
  readonly timezone: string
  readonly configuration: Values
  readonly applications: readonly AssignmentRequest[]
}

/**
 * A client's product state as the control plane resolved it: the client
 * itself, its product configuration, each assignment's components and
 * navigation resolved from the release it points to, and identity
 * reconciliation.
 */
export interface ClientProductResponse {
  readonly client: Client
  readonly product: ClientProduct
  readonly resolved: readonly {
    readonly applicationId: string
    readonly components: readonly ApplicationComponent[]
    readonly navigation: readonly NavigationItem[]
  }[]
  readonly reconciliation: Reconciliation
}
