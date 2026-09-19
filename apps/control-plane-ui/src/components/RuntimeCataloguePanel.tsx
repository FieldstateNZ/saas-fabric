import { useRuntimeCatalogue } from '../hooks/useRuntimeCatalogue'

/**
 * The derived runtime catalogue this environment's runtime would be given
 * right now (ADR 0023 part 3): every resource, its definition, and which
 * application release it comes from -- or the conflict that stops it from
 * being derived.
 *
 * Read-only, like `Components`: this panel surveys what publishing has
 * already produced. An operator changes a resource's own definition from
 * the application's Resources tab, not here.
 */
export function RuntimeCataloguePanel() {
  const catalogue = useRuntimeCatalogue()

  if (catalogue.loading) {
    return <p className="empty">Loading runtime catalogue…</p>
  }

  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Runtime catalogue</h2>
      </header>

      <div className="panel-body">
        {catalogue.value === null ? (
          <p className="error" role="alert">
            {catalogue.loadError}
          </p>
        ) : catalogue.value.resources.length === 0 ? (
          <p className="empty">No application release declares a resource yet.</p>
        ) : (
          <div className="table-wrap">
            <table className="fabric-table">
              <thead>
                <tr>
                  <th>Resource</th>
                  <th>Application</th>
                  <th>Version</th>
                  <th>Data source</th>
                  <th>Collection</th>
                  <th>Key field</th>
                  <th>Operations</th>
                  <th>Queryable fields</th>
                </tr>
              </thead>
              <tbody>
                {catalogue.value.resources.map((resource) => (
                  <tr key={resource.name}>
                    <td>{resource.name}</td>
                    <td>{resource.application}</td>
                    <td>{resource.version}</td>
                    <td className="mono">{resource.dataSource}</td>
                    <td className="mono">{resource.collection}</td>
                    <td className="mono">{resource.keyField}</td>
                    <td>{resource.operations.join(', ')}</td>
                    <td>{resource.queryableFields.join(', ') || '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </section>
  )
}
