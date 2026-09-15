import { useState } from 'react'

import type { EnvironmentRegistration } from '../api/catalogue-types'
import { Panel } from '../console/Panel'
import { PlatformViews } from '../console/PlatformViews'
import type { PlatformState } from '../hooks/usePlatform'
import { Field } from './Field'
import { SaveNotice } from './SaveNotice'
import type { CatalogueState } from './useCatalogue'

/** A blank registration, opened when an operator starts registering a new environment. */
const empty: EnvironmentRegistration = { id: '', name: '', consoleUrl: '', description: '' }

/**
 * The environment this deployment manages (see {@link PlatformViews}), and
 * links to every other independently authenticated operator console.
 *
 * Registering a link does not provision or connect anything — the control
 * plane holds no credential for another deployment's console, so this is a
 * directory an operator maintains by hand, not a control that reaches out to
 * verify what it points at.
 */
export function Environments({
  state,
  platform,
}: {
  state: CatalogueState
  platform: PlatformState
}) {
  const [editing, setEditing] = useState<EnvironmentRegistration | null>(null)
  const [success, setSuccess] = useState<string | null>(null)

  return (
    <>
      <PlatformViews platform={platform} environments />
      <SaveNotice error={state.error} success={success} onReload={state.refresh} />
      <div className="collection-heading">
        <h2>Registered environments</h2>
        <button
          onClick={() => {
            setEditing(empty)
          }}
        >
          + Register environment
        </button>
      </div>
      {editing && (
        <form
          className="panel panel-body"
          onSubmit={(event) => {
            event.preventDefault()
            void state.save({ action: 'saveEnvironment', environment: editing }).then((saved) => {
              if (saved) {
                setEditing(null)
                setSuccess('Environment registered.')
              }
            })
          }}
        >
          <fieldset disabled={state.saving}>
            <div className="form-grid">
              <Field
                label="Environment ID"
                required
                value={editing.id}
                onChange={(id) => {
                  setEditing({ ...editing, id })
                }}
              />
              <Field
                label="Environment name"
                required
                value={editing.name}
                onChange={(name) => {
                  setEditing({ ...editing, name })
                }}
              />
              <Field
                label="Console URL"
                required
                type="url"
                value={editing.consoleUrl}
                onChange={(consoleUrl) => {
                  setEditing({ ...editing, consoleUrl })
                }}
                hint="An HTTPS operator-console origin, such as https://console.example.com."
              />
              <Field
                label="Description"
                value={editing.description}
                onChange={(description) => {
                  setEditing({ ...editing, description })
                }}
              />
            </div>
            <div className="form-actions">
              <button className="primary-button" type="submit">
                Save environment
              </button>
              <button
                type="button"
                onClick={() => {
                  setEditing(null)
                }}
              >
                Cancel
              </button>
            </div>
          </fieldset>
        </form>
      )}
      {state.value?.catalogue.environments.map((environment) => (
        <Panel key={environment.id} title={environment.name}>
          <div className="panel-body">
            <p>{environment.description}</p>
            <div className="form-actions">
              <a href={environment.consoleUrl} target="_blank" rel="noreferrer">
                Open operator console ↗
              </a>
              <button
                onClick={() => {
                  setEditing(environment)
                }}
              >
                Edit registration
              </button>
            </div>
          </div>
        </Panel>
      ))}
    </>
  )
}
