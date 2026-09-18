import type {
  ConnectionSelector,
  DataSource,
  DataSourceInput,
  PlacementClass,
  ReportedConnection,
} from '../api/data-source-types'

/** The pool defaults ADR 0023's own example declaration uses. */
const DEFAULT_POOL = { maxConnections: '20', idleTimeoutSeconds: '300', acquireTimeoutSeconds: '5' }

/** Every placement class the form offers, and the label an operator reads for each. */
export const PLACEMENT_OPTIONS: readonly { value: PlacementClass; label: string }[] = [
  { value: 'shared', label: 'Shared' },
  { value: 'dedicated', label: 'Dedicated' },
  { value: 'highAvailability', label: 'High availability' },
  { value: 'regulated', label: 'Regulated' },
  { value: 'development', label: 'Development' },
  { value: 'ephemeral', label: 'Ephemeral' },
]

/** The label an operator reads for a placement class, wherever one is shown outside the form itself. */
export function placementLabel(placement: PlacementClass): string {
  return PLACEMENT_OPTIONS.find((option) => option.value === placement)?.label ?? placement
}

/**
 * Whether a connection the control plane reported is one ADR 0023 part 1
 * lets an operator declare.
 *
 * The platform refuses a third connection kind at declaration and
 * revalidates every held entry on read, so this should always be true. The
 * console still checks: `DataSourceRow` and `draftFrom` call this rather
 * than trusting `connection.kind` is `named` or `secret`, so a kind a hand
 * edit to the platform repository put there is never silently read as a
 * `secret` with an `undefined` reference.
 */
export function isDeclarableConnection(connection: ReportedConnection): connection is ConnectionSelector {
  return connection.kind === 'named' || connection.kind === 'secret'
}

/**
 * How a connection reads in the table, or why it cannot be shown as one --
 * for a row a hand edit to the platform repository put outside what ADR
 * 0023 part 1 lets an operator declare.
 */
export function connectionLabel(connection: ReportedConnection): string {
  if (!isDeclarableConnection(connection)) {
    return 'Not declarable'
  }

  return connection.kind === 'named' ? `Named: ${connection.name}` : `Secret: ${connection.reference}`
}

/**
 * Every field {@link DataSourceForm} edits, before it becomes a request.
 *
 * Numbers and the connection's single free-text value stay text here, the
 * way {@link Field} always holds text: a pool value mid-edit (a blank, a
 * leading zero) is a string an operator is still typing, not yet the number
 * `toInput` will send.
 */
export interface DataSourceDraft {
  readonly connector: string
  readonly connectionKind: 'named' | 'secret'
  readonly connectionValue: string
  readonly placement: PlacementClass
  readonly region: string
  readonly jurisdiction: string
  readonly maxConnections: string
  readonly idleTimeoutSeconds: string
  readonly acquireTimeoutSeconds: string
  readonly writable: boolean
  readonly acceptsNewTenants: boolean
  readonly discriminatorColumn: string
  readonly labels: Readonly<Record<string, string>>
}

/** A blank draft for a new declaration, or one pre-filled from the row being corrected. */
export function draftFrom(existing: DataSource | null): DataSourceDraft {
  if (existing === null) {
    return {
      connector: '',
      connectionKind: 'named',
      connectionValue: '',
      placement: 'shared',
      region: '',
      jurisdiction: '',
      ...DEFAULT_POOL,
      writable: true,
      acceptsNewTenants: true,
      discriminatorColumn: '',
      labels: {},
    }
  }

  // A not-declarable row has Edit disabled (see `DataSourceRow`), so this
  // fallback should be unreachable through the UI. It exists so a draft can
  // still be built honestly, rather than reading `.name`/`.reference` off a
  // shape this connection is not.
  const connection: ConnectionSelector = isDeclarableConnection(existing.connection)
    ? existing.connection
    : { kind: 'named', name: '' }

  return {
    connector: existing.connector,
    connectionKind: connection.kind,
    connectionValue: connection.kind === 'named' ? connection.name : connection.reference,
    placement: existing.placement,
    region: existing.residency.region,
    jurisdiction: existing.residency.jurisdiction ?? '',
    maxConnections: String(existing.pool.maxConnections),
    idleTimeoutSeconds: String(existing.pool.idleTimeoutSeconds),
    acquireTimeoutSeconds: String(existing.pool.acquireTimeoutSeconds),
    writable: existing.capabilities.writable,
    acceptsNewTenants: existing.capabilities.acceptsNewTenants,
    discriminatorColumn: existing.discriminator?.column ?? '',
    labels: existing.labels,
  }
}

/**
 * What the form sends: the wire shape ADR 0023 part 1 defines, built from
 * the draft's text fields. `discriminator` is dropped for anything but
 * `shared`, whatever text is still sitting in the (hidden) field -- ADR 0006
 * refuses one on any other placement, and this way the console never sends
 * a value it already knows will be refused.
 */
export function toInput(draft: DataSourceDraft): DataSourceInput {
  return {
    connector: draft.connector,
    connection:
      draft.connectionKind === 'named'
        ? { kind: 'named', name: draft.connectionValue }
        : { kind: 'secret', reference: draft.connectionValue },
    placement: draft.placement,
    residency: {
      region: draft.region,
      jurisdiction: draft.jurisdiction.trim() === '' ? null : draft.jurisdiction,
    },
    pool: {
      maxConnections: Number(draft.maxConnections),
      idleTimeoutSeconds: Number(draft.idleTimeoutSeconds),
      acquireTimeoutSeconds: Number(draft.acquireTimeoutSeconds),
    },
    capabilities: { writable: draft.writable, acceptsNewTenants: draft.acceptsNewTenants },
    discriminator: draft.placement === 'shared' ? { column: draft.discriminatorColumn } : null,
    labels: draft.labels,
  }
}
