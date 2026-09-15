import { useEffect, useState } from 'react'
import { getOperator } from '../api/catalogue'
import { PageHeader, Panel } from '../console/primitives'
import { FieldDefinitions } from './FieldDefinitions'
import { Field, SaveNotice } from './Forms'
import type { CatalogueState } from './useCatalogue'
export function DefinitionEditor({ state }: { state: CatalogueState }) {
  const [fields, setFields] = useState(state.value?.catalogue.clientFields ?? [])
  const [success, setSuccess] = useState<string | null>(null)
  useEffect(() => { setFields(state.value?.catalogue.clientFields ?? []) }, [state.value])
  return <><PageHeader title="Client definition" description={`Shared client configuration · version ${String(state.value?.catalogue.definitionVersion ?? 0)}`} />
    <SaveNotice error={state.error} success={success} onReload={state.refresh} />
    <Panel title="Core fields"><div className="panel-body"><p>Client ID, display name, legal name, region, timezone and hostnames are part of every client. Define additional non-secret fields below.</p></div></Panel>
    <form onSubmit={(event) => { event.preventDefault(); void state.save({ action: 'saveDefinition', fields }).then((saved) => { if (saved) setSuccess('Client definition saved. Existing clients keep their current values until reconfigured.') }) }}><fieldset disabled={state.saving}>
      <FieldDefinitions fields={fields} onChange={setFields} /><button className="primary-button" type="submit">{state.saving ? 'Saving…' : 'Save client definition'}</button></fieldset></form></>
}
export function SettingsEditor({ state }: { state: CatalogueState }) {
  const [settings, setSettings] = useState(state.value?.catalogue.settings ?? { platformName: '', defaultRegion: '', timezone: '' })
  const [operator, setOperator] = useState('Loading…')
  const [success, setSuccess] = useState<string | null>(null)
  useEffect(() => { if (state.value) setSettings(state.value.catalogue.settings) }, [state.value])
  useEffect(() => { let active = true; void getOperator().then((value) => { if (active) setOperator(value.subject) }, () => { if (active) setOperator('Unavailable') }); return () => { active = false } }, [])
  return <><PageHeader title="Settings" description="Platform identity and defaults for new clients." /><SaveNotice error={state.error} success={success} onReload={state.refresh} />
    <form onSubmit={(event) => { event.preventDefault(); void state.save({ action: 'saveSettings', settings }).then((saved) => { if (saved) setSuccess('Settings saved.') }) }}><fieldset disabled={state.saving}><Panel title="Platform settings"><div className="panel-body form-grid">
      <Field label="Platform name" required value={settings.platformName} onChange={(platformName) => { setSettings({ ...settings, platformName }) }} />
      <Field label="Default region" required value={settings.defaultRegion} onChange={(defaultRegion) => { setSettings({ ...settings, defaultRegion }) }} />
      <Field label="Default timezone" required value={settings.timezone} onChange={(timezone) => { setSettings({ ...settings, timezone }) }} />
    </div></Panel><button className="primary-button" type="submit">{state.saving ? 'Saving…' : 'Save settings'}</button></fieldset></form>
    <Panel title="Operator access"><div className="panel-body"><p>Signed in as <strong>{operator}</strong></p><p>Your organisation manages authentication and operator access.</p><a href="#/integrations">Manage integrations →</a></div></Panel></>
}
