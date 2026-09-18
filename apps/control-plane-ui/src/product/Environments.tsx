import { useState } from 'react'

import type { EnvironmentRegistration } from '../api/catalogue-types'
import { DataSourcesPanel } from '../components/DataSourcesPanel'
import { Panel } from '../console/Panel'
import { PlatformViews } from '../console/PlatformViews'
import type { PlatformState } from '../hooks/usePlatform'
import { EnvironmentRegistrationForm } from './EnvironmentRegistrationForm'
import { SaveNotice } from './SaveNotice'
import type { CatalogueState } from './useCatalogue'

/** A blank registration, opened when an operator starts registering a new environment. */
const empty: EnvironmentRegistration = { id: '', name: '', consoleUrl: '', description: '' }

/**
 * The environment this deployment manages (see {@link PlatformViews}), the
 * data sources it declares for placing a tenant's data (see
 * `DataSourcesPanel`, ADR 0023 part 1), and links to every other
 * independently authenticated operator console.
 *
 * Registering a link does not provision or connect anything -- the control
 * plane holds no credential for another deployment's console, so this is a
 * directory an operator maintains by hand, not a control that reaches out to
 * verify what it points at.
 *
 * The registration form lives in `EnvironmentRegistrationForm`, split out
 * once this file grew past a comfortable size for it: the platform view, the
 * data sources panel, and the list of already-registered environments are
 * already one page's worth of concerns without also carrying the form's
 * own field-by-field wiring.
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
      <DataSourcesPanel />
      <SaveNotice
        error={state.saveError}
        success={success}
        onReload={state.conflict ? state.refresh : undefined}
      />
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
        <EnvironmentRegistrationForm
          environment={editing}
          onChange={setEditing}
          saving={state.saving}
          onCancel={() => {
            setEditing(null)
          }}
          onSubmit={() => {
            void state.save({ action: 'saveEnvironment', environment: editing }).then((saved) => {
              if (saved) {
                setEditing(null)
                setSuccess('Environment registered.')
              }
            })
          }}
        />
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
