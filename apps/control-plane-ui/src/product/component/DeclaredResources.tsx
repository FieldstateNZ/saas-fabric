import type { ApplicationResource } from '../../api/catalogue-types'

/**
 * Data API resources a component descriptor declares, read-only and in
 * full: every operation it permits, not a summary, because publishing puts
 * each one in effect for every client of the application.
 */
export function DeclaredResources({
  resources,
}: {
  readonly resources: readonly ApplicationResource[]
}) {
  if (resources.length === 0) {
    return <p className="support-note">Declares no Data API resources.</p>
  }

  return (
    <div className="table-wrap">
      <table className="fabric-table">
        <thead>
          <tr>
            <th>Resource</th>
            <th>Logical data source</th>
            <th>Collection</th>
            <th>Key field</th>
            <th>Operations</th>
            <th>Queryable fields</th>
          </tr>
        </thead>
        <tbody>
          {resources.map((resource) => (
            <tr key={resource.name}>
              <td className="mono">{resource.name}</td>
              <td className="mono">{resource.dataSource}</td>
              <td className="mono">{resource.collection}</td>
              <td className="mono">{resource.keyField}</td>
              <td>{resource.operations.join(', ')}</td>
              <td>
                {resource.queryableFields.length === 0 ? '—' : resource.queryableFields.join(', ')}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
