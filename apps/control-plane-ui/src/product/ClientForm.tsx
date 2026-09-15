import { useState } from 'react'
import { createClient, saveProduct } from '../api/catalogue'
import type { Catalogue, ClientProductRequest, ClientProductResponse } from '../api/catalogue-types'
import { clientHref } from '../console/navigation'
import { PageHeader } from '../console/primitives'
import { describe } from '../hooks/useClients'
import { Assignments } from './Assignments'
import { ConfigurationInputs } from './ConfigurationInputs'
import { Field, SaveNotice } from './Forms'
export function ClientForm({ catalogue, existing, onSaved, onCancel }: { catalogue: Catalogue; existing?: ClientProductResponse; onSaved: () => void; onCancel?: () => void }) {
  const [id, setId] = useState(existing?.client.id ?? '')
  const [value, setValue] = useState<ClientProductRequest>(() => existing ? {
    displayName: existing.client.displayName, hosts: [...existing.client.hosts], legalName: existing.product.legalName, region: existing.product.region || catalogue.settings.defaultRegion,
    timezone: existing.product.timezone || catalogue.settings.timezone, configuration: existing.product.configuration,
    applications: existing.product.applications.map((a) => ({ applicationId: a.applicationId, version: a.release.version, planId: a.planId, configuration: a.configuration })),
  } : { displayName: '', legalName: '', hosts: [], region: catalogue.settings.defaultRegion, timezone: catalogue.settings.timezone, configuration: {}, applications: [] })
  const [hostText, setHostText] = useState(existing?.client.hosts.join(', ') ?? '')
  const [step, setStep] = useState(0)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  async function submit() {
    setBusy(true); setError(null)
    try { if (existing) await saveProduct(id, existing.client.revision, value); else await createClient(id, value)
      onSaved(); if (!existing) window.location.hash = clientHref(id).slice(1)
    } catch (error: unknown) { setError(describe(error)) } finally { setBusy(false) }
  }
  return <><PageHeader eyebrow="Clients" title={existing ? `Configure ${existing.client.displayName}` : 'Create client'} description="Set up the client, choose published applications, and review before saving." />
    <SaveNotice error={error} success={null} />
    <ol className="wizard-steps">{['Client details', 'Applications', 'Review'].map((name, index) => <li key={name} aria-current={step === index ? 'step' : undefined}>{index + 1}. {name}</li>)}</ol>
    <form onSubmit={(event) => { event.preventDefault(); if (step < 2) setStep(step + 1); else void submit() }}><fieldset disabled={busy}>
      {step === 0 && <div className="panel panel-body"><div className="form-grid">{!existing && <Field label="Client ID" required value={id} onChange={setId} hint="Permanent identifier: lowercase letters, numbers and hyphens." />}
        <Field label="Display name" required value={value.displayName} onChange={(displayName) => { setValue({ ...value, displayName }) }} />
        <Field label="Legal name" required value={value.legalName} onChange={(legalName) => { setValue({ ...value, legalName }) }} />
        <Field label="Region" required value={value.region} onChange={(region) => { setValue({ ...value, region }) }} />
        <Field label="Timezone" required value={value.timezone} onChange={(timezone) => { setValue({ ...value, timezone }) }} />
        <Field label="Hostnames (comma separated)" value={hostText} onChange={(hosts) => { setHostText(hosts); setValue({ ...value, hosts: hosts.split(',').map((h) => h.trim()).filter(Boolean) }) }} />
      </div><ConfigurationInputs fields={catalogue.clientFields} values={value.configuration} onChange={(configuration) => { setValue({ ...value, configuration }) }} /></div>}
      {step === 1 && <Assignments apps={catalogue.applications} value={value.applications} locked={existing?.product.applications.map((a) => a.applicationId) ?? []} onChange={(applications) => { setValue({ ...value, applications }) }} />}
      {step === 2 && <section className="panel panel-body"><h2>{value.displayName}</h2><dl className="facts"><dt>Client ID</dt><dd>{id}</dd><dt>Legal name</dt><dd>{value.legalName}</dd><dt>Region</dt><dd>{value.region}</dd><dt>Timezone</dt><dd>{value.timezone}</dd><dt>Hostnames</dt><dd>{value.hosts.join(', ') || 'None'}</dd></dl>
        <h3>Application assignments</h3>{value.applications.length === 0 ? <p>No applications selected.</p> : value.applications.map((assignment) => <p key={assignment.applicationId}>{assignment.applicationId} · {assignment.planId} · definition v{assignment.version}</p>)}
        <p>Saving writes desired state. Identity reconciliation follows separately; deployment and routing remain pending until observed.</p></section>}
      <div className="form-actions">{step > 0 && <button type="button" onClick={() => { setStep(step - 1) }}>Back</button>}
        <button type="submit" className="primary-button">{busy ? 'Saving…' : step < 2 ? 'Continue' : existing ? 'Save client configuration' : 'Create client'}</button>
        <button type="button" onClick={() => { if (onCancel) onCancel(); else window.location.hash = '/clients' }}>Cancel</button></div>
    </fieldset></form>
  </>
}
