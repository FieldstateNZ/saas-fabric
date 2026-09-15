import { useState } from 'react'

import { EmptyState } from '../console/EmptyState'
import { PageHeader } from '../console/PageHeader'
import { Panel } from '../console/Panel'
import { Status } from '../console/Status'
import { Field } from './Field'
import { SaveNotice } from './SaveNotice'
import type { CatalogueState } from './useCatalogue'

/**
 * The Applications page: every application in the catalogue, search, and
 * where an operator starts a new one.
 *
 * A new application starts as an unpublished draft — its ID is fixed at
 * creation (the form says so) because it becomes part of every URL and
 * request that names the application from here on, but nothing about the
 * application itself is locked in until it is published; see
 * `ApplicationWorkspace` for that boundary.
 */
export function ApplicationCatalogue({ state }: { state: CatalogueState }) {
  const [search, setSearch] = useState('')
  const [creating, setCreating] = useState(false)
  const [name, setName] = useState('')
  const [id, setId] = useState('')

  async function create() {
    if (await state.save({ action: 'createApplication', id, name })) {
      window.location.hash = `/applications/${encodeURIComponent(id)}`
    }
  }

  const apps =
    state.value?.catalogue.applications.filter((app) =>
      `${app.id} ${app.draft.name}`.toLowerCase().includes(search.toLowerCase()),
    ) ?? []

  return (
    <>
      <PageHeader
        title="Applications"
        description="Define the products your clients use, from components to plans and releases."
        actions={
          <button
            className="primary-button"
            onClick={() => {
              setCreating(!creating)
            }}
          >
            + New application
          </button>
        }
      />
      <SaveNotice error={state.error} success={null} onReload={state.refresh} />
      {creating && (
        <form
          className="panel panel-body"
          onSubmit={(event) => {
            event.preventDefault()
            void create()
          }}
        >
          <h2>New application</h2>
          <fieldset disabled={state.saving}>
            <div className="form-grid">
              <Field label="Application name" required value={name} onChange={setName} />
              <Field
                label="Application ID"
                required
                value={id}
                onChange={setId}
                hint="Lowercase letters, numbers and hyphens. This cannot change later."
              />
            </div>
            <div className="form-actions">
              <button className="primary-button" type="submit">
                {state.saving ? 'Creating…' : 'Create application'}
              </button>
              <button
                type="button"
                onClick={() => {
                  setCreating(false)
                }}
              >
                Cancel
              </button>
            </div>
          </fieldset>
        </form>
      )}
      <div className="toolbar">
        <label className="search">
          <input
            aria-label="Search applications"
            placeholder="Search applications…"
            value={search}
            onChange={(event) => {
              setSearch(event.target.value)
            }}
          />
        </label>
      </div>
      {state.loading ? (
        <p role="status">Loading applications…</p>
      ) : apps.length === 0 ? (
        <EmptyState title="No applications yet">
          <p>Create an application, define its plans, then publish it for clients to use.</p>
        </EmptyState>
      ) : (
        <div className="application-grid">
          {apps.map((app) => (
            <Panel
              key={app.id}
              title={app.draft.name}
              action={
                <Status value={app.releases.length ? 'applied' : 'neutral'}>
                  {app.releases.length
                    ? `v${String(app.releases.at(-1)?.version)} published`
                    : 'Draft'}
                </Status>
              }
            >
              <div className="panel-body">
                <p>{app.draft.description || 'No description yet.'}</p>
                <p className="support-note">
                  {app.draft.components.length} components · {app.draft.features.length} features ·{' '}
                  {app.draft.plans.length} plans
                </p>
                <a href={`#/applications/${encodeURIComponent(app.id)}`}>Manage application →</a>
              </div>
            </Panel>
          ))}
        </div>
      )}
    </>
  )
}
