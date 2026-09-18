/**
 * The shapes the control-plane API speaks about data sources (ADR 0023
 * part 1).
 *
 * Split from `./types` the way `./catalogue-types` already is: this
 * describes what an environment declares it can place a tenant's data on,
 * a different conversation from a client's own identity and reconciliation.
 */

/**
 * How a data source's connector reaches it, restricted to what ADR 0023
 * part 1 lets an operator declare.
 *
 * Never a value: ADR 0023 refuses to let a credential be declared through
 * this form, so the only things a connection can carry are a name the
 * connector already holds or a reference to where a secret lives. The wire
 * also carries a third shape -- the connector's single default connection --
 * but the platform refuses it at declaration, so it is never something this
 * console builds. See {@link ReportedConnection} for what a *read* connection
 * is typed as.
 */
export type ConnectionSelector =
  | { readonly kind: 'named'; readonly name: string }
  | { readonly kind: 'secret'; readonly reference: string }

/**
 * How a data source's connector reaches it, as the control plane reports it
 * back on a read.
 *
 * The platform refuses a third connection shape at declaration and
 * revalidates every held entry when it answers a read, so in practice this
 * should always be a {@link ConnectionSelector}. This type does not assume
 * that enforcement can never be bypassed by a hand edit to the platform
 * repository, though -- narrowing straight to `ConnectionSelector` would
 * coerce anything unexpected into looking like a `secret` with an
 * `undefined` reference, silently. `DataSourceRow` and `draftFrom` narrow
 * this honestly instead, through `isDeclarableConnection`, and treat
 * anything outside the two known kinds as not declarable.
 */
export type ReportedConnection = ConnectionSelector | { readonly kind: string }

/**
 * What a data source is for, and who may be placed on it.
 *
 * Every placement but `shared` cannot be placed on yet -- ADR 0023 part 1
 * only declares the data source, and the control plane refuses a placement
 * it cannot yet provision. Declaring one anyway is still useful: it states
 * that the database exists.
 */
export type PlacementClass =
  | 'shared'
  | 'dedicated'
  | 'highAvailability'
  | 'regulated'
  | 'development'
  | 'ephemeral'

/** Where a data source's rows must legally stay. */
export interface DataResidency {
  readonly region: string
  readonly jurisdiction: string | null
}

/** Connection-pool limits. `20 / 300 / 5` are the defaults ADR 0023 itself uses. */
export interface PoolSettings {
  readonly maxConnections: number
  readonly idleTimeoutSeconds: number
  readonly acquireTimeoutSeconds: number
}

/** What this data source currently permits. */
export interface DataSourceCapabilities {
  readonly writable: boolean
  readonly acceptsNewTenants: boolean
}

/**
 * The column every collection on a shared data source carries.
 *
 * Required when `placement` is `shared` and refused on any other placement
 * (ADR 0006, ADR 0023 part 1) -- it is the only isolation a shared source may
 * serve, so a data source that is not shared has no use for one.
 */
export interface Discriminator {
  readonly column: string
}

/**
 * One data source this environment declares -- the model ADR 0023 part 1
 * gives the control plane, not a connector's runtime state. `revision` is
 * this entry's own `BindingRevision`, bumped by Fabric on every declared
 * change; it is not the document revision the console conditions writes on
 * (see {@link DataSources.revision}).
 */
export interface DataSource {
  readonly id: string
  readonly revision: number
  readonly connector: string
  readonly connection: ReportedConnection
  readonly placement: PlacementClass
  readonly residency: DataResidency
  readonly pool: PoolSettings
  readonly capabilities: DataSourceCapabilities
  readonly discriminator: Discriminator | null
  readonly labels: Readonly<Record<string, string>>
}

/**
 * What an operator submits to declare or correct one data source.
 *
 * Everything {@link DataSource} carries except `id` (the path names it) and
 * `revision` (the control plane computes it -- see ADR 0023 part 1, "the
 * incoming declaration's revision is ignored and computed"). `connection`
 * is narrowed back to {@link ConnectionSelector}: a read can report a kind
 * outside `named`/`secret` honestly, but a declaration can never build one.
 */
export type DataSourceInput = Omit<DataSource, 'id' | 'revision' | 'connection'> & {
  readonly connection: ConnectionSelector
}

/**
 * Every data source declared for this deployment's one environment, and the
 * document revision to condition the next write on.
 *
 * `revision` is never null, even for an environment that has declared
 * nothing yet: the late-bound binding always has a fact to compare-and-swap
 * on -- the generation tag -- so an empty environment still reads one, and
 * the very first declaration conditions its `If-Match` on exactly that tag,
 * the same as every later one (see `declareDataSource`).
 */
export interface DataSources {
  readonly environment: string
  readonly revision: string
  readonly dataSources: readonly DataSource[]
}
