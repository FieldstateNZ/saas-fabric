/**
 * The shapes the control-plane API speaks about the derived runtime
 * catalogue (ADR 0023 part 3).
 *
 * Split from `./catalogue-types` the way `./data-source-types` already is:
 * this describes what a runtime in this environment would be given right
 * now, derived from every application's newest published release -- not
 * what an operator edits. An operator edits resources on an application's
 * own draft; see `ApplicationResource` in `./catalogue-types`.
 */

import type { OperationKind } from './catalogue-types'

/**
 * One resource the runtime catalogue derives, and which application release
 * it comes from.
 *
 * For a given `name`, the derivation always picks the newest published
 * release of whichever application declares it -- a newer release
 * supersedes an older definition of the same resource, and a client still
 * pinned to an older release sees the newer shape (ADR 0023 §3). `version`
 * names which release this definition was taken from.
 */
export interface RuntimeResource {
  readonly name: string
  readonly application: string
  readonly version: number
  readonly dataSource: string
  readonly collection: string
  readonly keyField: string
  readonly operations: readonly OperationKind[]
  readonly queryableFields: readonly string[]
}

/**
 * The derived runtime catalogue: every resource a runtime in this
 * environment would be given right now, and the catalogue revision it was
 * derived from.
 *
 * Two different applications declaring the same resource name is a
 * conflict Fabric's own writes can never produce -- publishing refuses it --
 * so the only way to reach one is a hand edit to the stored catalogue.
 * `getRuntimeCatalogue` reports that as `500 desired_state_invalid`, the
 * same answer a client document that will not parse gets; there is no
 * partial `RuntimeCatalogue` in that case; see `useRuntimeCatalogue`.
 */
export interface RuntimeCatalogue {
  readonly revision: string
  readonly resources: readonly RuntimeResource[]
}
