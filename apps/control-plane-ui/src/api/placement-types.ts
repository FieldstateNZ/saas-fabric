/**
 * The shapes the control-plane API speaks about a client's data placement
 * (ADR 0023 part 2).
 *
 * Split from `./data-source-types` the way that file is split from
 * `./types`: this describes what a client's document asks for and where
 * Fabric put it, a different conversation from what an environment declares
 * it can place a tenant's data on.
 */
import type { PlacementClass } from './data-source-types'

/**
 * How a tenant's rows are told apart on the data source they were placed
 * on, exactly as Fabric recorded it (ADR 0023 part 2).
 *
 * `schema` is part of the wire contract though nothing places one yet (ADR
 * 0006: inert) -- this type still carries it honestly, rather than assuming
 * a placement record can never report it.
 */
export type IsolationModel =
  | { readonly kind: 'database' }
  | { readonly kind: 'schema'; readonly schema: string }
  | { readonly kind: 'discriminator'; readonly column: string; readonly value: string }

/**
 * A client's intent for one logical data source -- `spec.data.<logical>` in
 * its document, carried here exactly as the client declared it.
 *
 * `provider` is carried and shown, never matched against anything: nothing a
 * declared data source states corresponds to it yet.
 */
export interface DataIntent {
  readonly class: PlacementClass
  readonly provider: string | null
  readonly region: string | null
}

/** Where an intent was placed, and when. */
export interface Placed {
  readonly dataSource: string
  readonly isolation: IsolationModel
  readonly placedAt: string
}

/**
 * One logical data source a client's document asks for: the intent, and
 * either where Fabric placed it or why it refused to.
 *
 * `placed` and `refusal` are both null when the intent is placeable but has
 * not been asked for yet -- the state that enables the `Place` button (see
 * `PlacementRow`).
 */
export interface PlacementEntry {
  readonly logical: string
  readonly intent: DataIntent
  readonly placed: Placed | null
  readonly refusal: string | null
}

/** A catalogue resource, and the application release that declares it. */
export interface UnboundResource {
  readonly resource: string
  readonly application: string
  readonly version: number
}

/**
 * A logical data source the runtime catalogue names and this client's tenant
 * does not bind, so every resource on it answers `UnboundDataSource` (500) to
 * this client (G8, ADR 0018 part 4).
 *
 * `declared` says whether the client's document asks for it: when it does,
 * placing that intent binds it; when it does not, the document has to ask
 * for it first.
 */
export interface UnboundDataSource {
  readonly logical: string
  readonly declared: boolean
  readonly resources: readonly UnboundResource[]
}

/**
 * Whether the client's placements were checked against the environment's
 * derived runtime catalogue. `unchecked` when the catalogue could not be read
 * or derived; `reason` says why, and placing data still works.
 */
export type UnboundCheck =
  | { readonly state: 'checked'; readonly dataSources: readonly UnboundDataSource[] }
  | { readonly state: 'unchecked'; readonly reason: string }

/**
 * Every logical data source one client's document asks for, the catalogue
 * resources its tenant cannot reach, and the placements document revision to
 * condition the next `place` on.
 *
 * `revision` is never null, for the same reason `DataSources.revision` is
 * not (see `./data-source-types`): the late-bound binding always has a
 * generation tag to compare-and-swap on, even before any placement has ever
 * been recorded for this client.
 */
export interface Placements {
  readonly clientId: string
  readonly environment: string
  readonly revision: string
  readonly placements: readonly PlacementEntry[]
  readonly unbound: UnboundCheck
}
