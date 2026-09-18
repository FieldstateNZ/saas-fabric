import { useState } from 'react'

import type { DataSource, DataSourceInput } from '../api/data-source-types'
import { useDataSources } from '../hooks/useDataSources'
import { SaveNotice } from '../product/SaveNotice'
import { DataSourceForm } from './DataSourceForm'
import { DataSourceRow } from './DataSourceRow'

/** Whether the form is closed, open for a new declaration, or open on a row being corrected. */
type FormState =
  | { readonly mode: 'closed' }
  | { readonly mode: 'creating' }
  | { readonly mode: 'editing'; readonly dataSource: DataSource }

/**
 * Every data source this environment declares (ADR 0023 part 1), and the
 * form that declares or corrects one.
 *
 * Renders nothing while this deployment manages no platform at all:
 * `Environments` already shows the connect prompt for that, through
 * `PlatformViews`, and a second empty state here would just repeat it.
 */
export function DataSourcesPanel() {
  const dataSources = useDataSources()
  const [form, setForm] = useState<FormState>({ mode: 'closed' })
  const [success, setSuccess] = useState<string | null>(null)

  if (dataSources.unmanaged) {
    return null
  }

  if (dataSources.loading) {
    return <p className="empty">Loading data sources…</p>
  }

  if (dataSources.value === null) {
    return (
      <p className="error" role="alert">
        {dataSources.loadError}
      </p>
    )
  }

  const declared = dataSources.value.dataSources

  function submit(id: string, input: DataSourceInput): void {
    // Cleared here, not only set on success -- a failure right after a
    // previous declare succeeded would otherwise still show that old
    // confirmation alongside the new refusal.
    setSuccess(null)
    void dataSources.declare(id, input).then((saved) => {
      if (saved) {
        setForm({ mode: 'closed' })
        setSuccess(`${id} declared.`)
      }
    })
  }

  return (
    <section className="panel">
      <header className="panel-header">
        <h2>Data sources</h2>
        <button
          type="button"
          onClick={() => {
            setForm({ mode: 'creating' })
          }}
        >
          + Declare data source
        </button>
      </header>

      <SaveNotice
        error={dataSources.saveError}
        success={success}
        onReload={dataSources.conflict ? dataSources.refresh : undefined}
      />

      {form.mode !== 'closed' && (
        // Keyed by target, so switching from one row's Edit to another's (or
        // to Declare) remounts the form with that target's values. The form
        // reads `existing` once, at mount; without the key React would keep
        // the same instance and the previous row's draft under a new heading.
        <DataSourceForm
          key={form.mode === 'editing' ? form.dataSource.id : 'declare'}
          existing={form.mode === 'editing' ? form.dataSource : null}
          saving={dataSources.saving}
          onSubmit={submit}
          onCancel={() => {
            setForm({ mode: 'closed' })
          }}
        />
      )}

      <div className="panel-body">
        {declared.length === 0 ? (
          <p className="empty">
            No data sources declared. Declare one to tell Fabric where a tenant's data can be
            placed.
          </p>
        ) : (
          <div className="table-wrap">
            <table className="fabric-table">
              <thead>
                <tr>
                  <th>ID</th>
                  <th>Connector</th>
                  <th>Connection</th>
                  <th>Placement</th>
                  <th>Region</th>
                  <th>Writable</th>
                  <th>Accepts new tenants</th>
                  <th>Discriminator</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {declared.map((dataSource) => (
                  <DataSourceRow
                    key={dataSource.id}
                    dataSource={dataSource}
                    onEdit={() => {
                      setForm({ mode: 'editing', dataSource })
                    }}
                  />
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </section>
  )
}
