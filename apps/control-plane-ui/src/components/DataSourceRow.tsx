import { useState } from 'react'

import type { DataSource } from '../api/data-source-types'
import { connectionLabel, isDeclarableConnection, placementLabel } from './data-source-draft'

/**
 * One declared data source, as the table shows it.
 *
 * `onRemove` is a two-step confirm rather than an immediate delete: clicking
 * `Remove` swaps that cell for "Remove <id>?" and a `Confirm`/`Cancel` pair,
 * so an operator cannot remove a data source with the same single click that
 * opens a menu. `confirming` resets itself once `onRemove`'s promise settles,
 * whatever it resolved to -- a success drops this row from the list the panel
 * re-renders from, and a refusal is reported by the panel's own `SaveNotice`,
 * so there is nothing left for this row's confirm state to hold open.
 */
export function DataSourceRow({
  dataSource,
  saving,
  onEdit,
  onRemove,
}: {
  dataSource: DataSource
  saving: boolean
  onEdit: () => void
  onRemove: () => Promise<boolean>
}) {
  const [confirming, setConfirming] = useState(false)
  const declarable = isDeclarableConnection(dataSource.connection)

  return (
    <tr>
      <td className="mono">{dataSource.id}</td>
      <td>{dataSource.connector}</td>
      <td>{connectionLabel(dataSource.connection)}</td>
      <td>{placementLabel(dataSource.placement)}</td>
      <td>{dataSource.residency.region}</td>
      <td>{dataSource.capabilities.writable ? 'Yes' : 'No'}</td>
      <td>{dataSource.capabilities.acceptsNewTenants ? 'Yes' : 'No'}</td>
      <td>{dataSource.discriminator?.column ?? '—'}</td>
      <td>
        <button
          type="button"
          onClick={onEdit}
          disabled={!declarable || saving}
          title={declarable ? undefined : 'Correct this connection in the platform repository before editing it here.'}
        >
          Edit
        </button>
        {confirming ? (
          <>
            <span>Remove {dataSource.id}?</span>
            <button
              type="button"
              disabled={saving}
              onClick={() => {
                void onRemove().then(() => {
                  setConfirming(false)
                })
              }}
            >
              Confirm
            </button>
            <button
              type="button"
              disabled={saving}
              onClick={() => {
                setConfirming(false)
              }}
            >
              Cancel
            </button>
          </>
        ) : (
          <button
            type="button"
            disabled={saving}
            onClick={() => {
              setConfirming(true)
            }}
          >
            Remove
          </button>
        )}
      </td>
    </tr>
  )
}
