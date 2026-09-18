import type { ClientProductResponse } from '../api/catalogue-types'
import { EmptyState } from '../console/EmptyState'
import { Panel } from '../console/Panel'
import { usePlacements } from '../hooks/usePlacements'
import { PlacementRow } from './PlacementRow'
import { SaveNotice } from './SaveNotice'

/**
 * The Data tab of `ClientWorkspace`: every logical data source this
 * client's document asks for (`spec.data.<logical>`), each either placed or
 * refused with the reason, and a way to ask Fabric to place it (ADR 0023
 * part 2).
 *
 * The tab body is the whole panel, so while this deployment manages no
 * platform at all it shows the same "Connect platform management"
 * `EmptyState` as `PlatformViews`' Environments view, phrased for placement
 * instead of a blank area with no explanation.
 *
 * A conflict (`409 revision_conflict`) is shown once, at the top, as the
 * reload affordance `SaveNotice` already gives every other conflict in this
 * console. A refusal Fabric can name (`422 placement_refused`) is shown
 * beside the row it is about instead -- see `usePlacements`'
 * `saveErrorLogical`.
 */
export function ClientDataTab({ data }: { data: ClientProductResponse }) {
  const placements = usePlacements(data.client.id)

  if (placements.unmanaged) {
    return (
      <EmptyState title="Connect platform management">
        <p>Placing this client&rsquo;s data needs Platform Management connected.</p>
        <a className="primary-link" href="#/integrations">
          Manage integrations →
        </a>
      </EmptyState>
    )
  }

  if (placements.loading) {
    return <p className="empty">Loading data placement…</p>
  }

  if (placements.value === null) {
    return (
      <p className="error" role="alert">
        {placements.loadError}
      </p>
    )
  }

  const rows = placements.value.placements

  return (
    <Panel title="Data">
      <SaveNotice
        error={placements.conflict ? placements.saveError : null}
        success={null}
        onReload={placements.conflict ? placements.refresh : undefined}
      />
      {rows.length === 0 ? (
        <p className="panel-body">This client's document declares no data sources.</p>
      ) : (
        <div className="table-wrap">
          <table className="fabric-table">
            <thead>
              <tr>
                <th>Logical</th>
                <th>Intent</th>
                <th>Placement</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <PlacementRow
                  key={row.logical}
                  placement={row}
                  saving={placements.saving}
                  error={
                    !placements.conflict && placements.saveErrorLogical === row.logical
                      ? placements.saveError
                      : null
                  }
                  onPlace={() => {
                    void placements.place(row.logical)
                  }}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Panel>
  )
}
