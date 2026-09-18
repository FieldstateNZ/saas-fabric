import type { DataSource } from '../api/data-source-types'
import { connectionLabel, isDeclarableConnection, placementLabel } from './data-source-draft'

/** One declared data source, as the table shows it. */
export function DataSourceRow({
  dataSource,
  onEdit,
}: {
  dataSource: DataSource
  onEdit: () => void
}) {
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
          disabled={!declarable}
          title={declarable ? undefined : 'Correct this connection in the platform repository before editing it here.'}
        >
          Edit
        </button>
      </td>
    </tr>
  )
}
