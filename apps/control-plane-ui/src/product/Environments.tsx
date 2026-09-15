import { useState } from 'react'
import type { EnvironmentRegistration } from '../api/catalogue-types'
import type { PlatformState } from '../hooks/usePlatform'
import { PlatformViews } from '../console/PlatformViews'
import { Panel } from '../console/primitives'
import { Field, SaveNotice } from './Forms'
import type { CatalogueState } from './useCatalogue'
const empty: EnvironmentRegistration = { id: '', name: '', consoleUrl: '', description: '' }
export function Environments({ state, platform }: { state: CatalogueState; platform: PlatformState }) {
  const [editing, setEditing] = useState<EnvironmentRegistration | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  return <><PlatformViews platform={platform} environments /><SaveNotice error={state.error} success={success} onReload={state.refresh} />
    <div className="collection-heading"><h2>Registered environments</h2><button onClick={() => { setEditing(empty) }}>+ Register environment</button></div>
    {editing && <form className="panel panel-body" onSubmit={(event) => { event.preventDefault(); void state.save({ action: 'saveEnvironment', environment: editing }).then((saved) => { if (saved) { setEditing(null); setSuccess('Environment registered.') } }) }}><fieldset disabled={state.saving}><div className="form-grid">
      <Field label="Environment ID" required value={editing.id} onChange={(id) => { setEditing({ ...editing, id }) }} /><Field label="Environment name" required value={editing.name} onChange={(name) => { setEditing({ ...editing, name }) }} />
      <Field label="Console URL" required type="url" value={editing.consoleUrl} onChange={(consoleUrl) => { setEditing({ ...editing, consoleUrl }) }} hint="An HTTPS operator-console origin, such as https://console.example.com." />
      <Field label="Description" value={editing.description} onChange={(description) => { setEditing({ ...editing, description }) }} /></div>
      <div className="form-actions"><button className="primary-button" type="submit">Save environment</button><button type="button" onClick={() => { setEditing(null) }}>Cancel</button></div></fieldset></form>}
    {state.value?.catalogue.environments.map((environment) => <Panel key={environment.id} title={environment.name}><div className="panel-body"><p>{environment.description}</p><div className="form-actions"><a href={environment.consoleUrl} target="_blank" rel="noreferrer">Open operator console ↗</a><button onClick={() => { setEditing(environment) }}>Edit registration</button></div></div></Panel>)}
  </>
}
