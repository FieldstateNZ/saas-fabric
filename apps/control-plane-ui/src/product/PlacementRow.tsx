import type { IsolationModel, PlacementEntry } from '../api/placement-types'
import { placementLabel } from '../components/data-source-draft'

/** How a placement's isolation reads in the table. */
function isolationLabel(isolation: IsolationModel): string {
  switch (isolation.kind) {
    case 'database':
      return 'Database'
    case 'schema':
      return `Schema: ${isolation.schema}`
    case 'discriminator':
      return `Discriminator: ${isolation.column}=${isolation.value}`
    default: {
      const exhaustive: never = isolation
      return exhaustive
    }
  }
}

/**
 * One logical data source, as the Data tab's table shows it: the intent,
 * where Fabric placed it (or why it refused to), and the `Place` button
 * that asks Fabric to (ADR 0023 part 2).
 *
 * `Place` is enabled only when `placed` and `refusal` are both null --
 * ADR 0023 part 2's placeable state -- because anything else already has an
 * answer: a placed row has nothing left to ask for, and a refused row would
 * be refused again by the same rule that refused it the first time, until
 * whatever it named (a data source, a region) is declared.
 */
export function PlacementRow({
  placement,
  saving,
  error,
  onPlace,
}: {
  placement: PlacementEntry
  saving: boolean
  /** The message beside this row for a refusal `place` itself received (422). */
  error: string | null
  onPlace: () => void
}) {
  const placeable = placement.placed === null && placement.refusal === null

  return (
    <tr>
      <td className="mono">{placement.logical}</td>
      <td>
        {placementLabel(placement.intent.class)}
        {placement.intent.provider !== null && ` · ${placement.intent.provider}`}
        {placement.intent.region !== null && ` · ${placement.intent.region}`}
      </td>
      <td>
        {placement.placed !== null && (
          <>
            <span className="mono">{placement.placed.dataSource}</span>
            {` · ${isolationLabel(placement.placed.isolation)} · `}
            {new Date(placement.placed.placedAt).toLocaleString()}
          </>
        )}
        {placement.placed === null && placement.refusal !== null && (
          <span className="error">{placement.refusal}</span>
        )}
        {placement.placed === null && placement.refusal === null && '—'}
        {error !== null && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
      </td>
      <td>
        <button type="button" disabled={!placeable || saving} onClick={onPlace}>
          Place
        </button>
      </td>
    </tr>
  )
}
