/**
 * The shapes the control-plane API speaks about an application's components
 * (ADR 0026 section 7): a component as the catalogue stores it, the
 * resolution the server records for a described one, the component
 * descriptor it froze, and what a save and a selection may say.
 *
 * Its own file because `./catalogue-types` is at its size limit, and because
 * a described component is a shape of its own there too: the Rust model
 * keeps it in `catalogue/component.rs` and `catalogue/draft_component.rs`.
 */
import type {
  ApplicationDefinition,
  ApplicationResource,
  ConfigurationField,
} from './catalogue-types'

/**
 * How a component is deployed, or that it is the platform's.
 *
 * `described` is a component selected by its primary image's repository and
 * a version tag, and resolved by the server through the component
 * descriptor attached to that image. The other three are authored as free
 * text, as they always were.
 */
export type ComponentKind = 'container' | 'helm' | 'capability' | 'described'

/** The closed list of what a component may need from the platform (ADR 0021). */
export type PlatformCapability =
  | 'Identity'
  | 'Database'
  | 'Secrets'
  | 'Authorization'
  | 'Routing'
  | 'Object storage'
  | 'Messaging'

/** One image of a component: where it lives and exactly which bytes it is. */
export interface ImageReference {
  readonly repository: string
  readonly digest: string
}

/**
 * What a component descriptor says (ADR 0026 section 2). Every section is
 * always present in a frozen copy: the server renders it canonically.
 */
export interface ComponentSpec {
  readonly name: string
  readonly title: string
  readonly description: string
  readonly version: string
  /** Every image, by role. */
  readonly images: Readonly<Record<string, ImageReference>>
  /** Needs, never a claim that anything provides them. */
  readonly capabilities: readonly PlatformCapability[]
  readonly fields: readonly ConfigurationField[]
  readonly resources: readonly ApplicationResource[]
}

/** A component descriptor, frozen whole -- its envelope included. */
export interface FrozenDescriptor {
  readonly apiVersion: string
  readonly kind: string
  readonly spec: ComponentSpec
}

/**
 * What the server observed when it resolved a described component: the
 * release unit it called complete, and a frozen copy of its component
 * descriptor. Only the server writes one; a save can never carry it.
 *
 * `resolvedAt` is when Fabric read it, in Unix seconds -- what Fabric
 * observed then, never a claim that the artifact is still there.
 */
export interface ComponentResolution {
  readonly repository: string
  readonly version: string
  readonly primaryDigest: string
  readonly descriptorDigest: string
  /** The one commit every image and the component descriptor name. */
  readonly revision: string
  readonly resolvedAt: number
  readonly descriptor: FrozenDescriptor
}

/**
 * One piece of an application definition: a container, a Helm chart, a
 * capability the platform itself already provides, or a described
 * component.
 *
 * `kind: 'capability'` is never deployed -- its `reference` names a platform
 * capability such as Identity or Secrets. For `kind: 'described'`,
 * `reference` and `version` are the resolution's repository and version
 * tag, and `resolution` is present; the server omits `resolution` for every
 * other kind, and a component added in this console has none until a
 * version is selected for it.
 */
export interface ApplicationComponent {
  readonly id: string
  readonly name: string
  readonly kind: ComponentKind
  readonly reference: string
  readonly version: string
  readonly required: boolean
  readonly policy: 'automatic' | 'manual'
  readonly resolution?: ComponentResolution
}

/** A `container`, `helm` or `capability` component as a save names it: today's shape. */
export interface AuthoredDraftComponent {
  readonly kind: 'container' | 'helm' | 'capability'
  readonly id: string
  readonly name: string
  readonly reference: string
  readonly version: string
  readonly required: boolean
  readonly policy: 'automatic' | 'manual'
}

/**
 * A described component as a save names it: only what an operator decides
 * about it. The server refuses a body carrying its reference, version or
 * resolution rather than ignoring them.
 */
export interface DescribedDraftComponent {
  readonly kind: 'described'
  readonly id: string
  readonly name: string
  readonly required: boolean
  readonly policy: 'automatic' | 'manual'
}

/** A component as a save names it, tagged by `kind`. */
export type DraftComponent = AuthoredDraftComponent | DescribedDraftComponent

/**
 * The body of a `saveApplication` command: a definition whose components
 * are in {@link DraftComponent}'s shape, so a save cannot forge what the
 * server resolved.
 */
export type ApplicationDraft = Omit<ApplicationDefinition, 'components'> & {
  readonly components: readonly DraftComponent[]
}

/**
 * Selecting a version of a component by its primary image's repository and
 * a version tag. It names no digest and nothing the component descriptor
 * says: the server resolves the version against the registry, and answers
 * with the catalogue it wrote. A component id that names nothing creates a
 * described component; one naming a container or chart converts it in
 * place.
 */
export interface SelectComponentVersion {
  readonly action: 'selectComponentVersion'
  readonly id: string
  readonly component: string
  readonly repository: string
  readonly version: string
}
