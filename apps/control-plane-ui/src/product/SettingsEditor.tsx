import { useEffect, useState } from 'react'

import { getOperator } from '../api/catalogue'
import { PageHeader } from '../console/PageHeader'
import { Panel } from '../console/Panel'
import { Field } from './Field'
import { SaveNotice } from './SaveNotice'
import type { CatalogueState } from './useCatalogue'

/**
 * Platform display defaults for new clients, and who is signed in.
 *
 * Operator access is shown, not managed, here: this console authenticates
 * operators but does not administer them — see `IdentityPanel` and
 * `IntegrationsPanel` for the boundary this console keeps around identity
 * providers in general.
 */
export function SettingsEditor({ state }: { state: CatalogueState }) {
  const [settings, setSettings] = useState(
    state.value?.catalogue.settings ?? { platformName: '', defaultRegion: '', timezone: '' },
  )
  const [operator, setOperator] = useState('Loading…')
  const [success, setSuccess] = useState<string | null>(null)

  useEffect(() => {
    if (state.value) {
      setSettings(state.value.catalogue.settings)
    }
  }, [state.value])

  useEffect(() => {
    let active = true

    void getOperator().then(
      (value) => {
        if (active) {
          setOperator(value.subject)
        }
      },
      () => {
        if (active) {
          setOperator('Unavailable')
        }
      },
    )

    return () => {
      active = false
    }
  }, [])

  return (
    <>
      <PageHeader title="Settings" description="Platform identity and defaults for new clients." />
      <SaveNotice
        error={state.saveError}
        success={success}
        onReload={state.conflict ? state.refresh : undefined}
      />
      <form
        onSubmit={(event) => {
          event.preventDefault()
          void state.save({ action: 'saveSettings', settings }).then((saved) => {
            if (saved) {
              setSuccess('Settings saved.')
            }
          })
        }}
      >
        <fieldset disabled={state.saving}>
          <Panel title="Platform settings">
            <div className="panel-body form-grid">
              <Field
                label="Platform name"
                required
                value={settings.platformName}
                onChange={(platformName) => {
                  setSettings({ ...settings, platformName })
                }}
              />
              <Field
                label="Default region"
                required
                value={settings.defaultRegion}
                onChange={(defaultRegion) => {
                  setSettings({ ...settings, defaultRegion })
                }}
              />
              <Field
                label="Default timezone"
                required
                value={settings.timezone}
                onChange={(timezone) => {
                  setSettings({ ...settings, timezone })
                }}
              />
            </div>
          </Panel>
          <button className="primary-button" type="submit">
            {state.saving ? 'Saving…' : 'Save settings'}
          </button>
        </fieldset>
      </form>
      <Panel title="Operator access">
        <div className="panel-body">
          <p>
            Signed in as <strong>{operator}</strong>
          </p>
          <p>Your organisation manages authentication and operator access.</p>
          <a href="#/integrations">Manage integrations →</a>
        </div>
      </Panel>
    </>
  )
}
