import { useEffect, useState } from 'react'
import { getProduct } from '../api/catalogue'
import type { Catalogue, ClientProductResponse } from '../api/catalogue-types'
import { IdentityPanel } from '../components/IdentityPanel'
import { Secrets } from '../components/tabs/Secrets'
import { PageHeader, Panel, Status } from '../console/primitives'
import { describe } from '../hooks/useClients'
import { ClientForm } from './ClientForm'
import { ClientShell } from './ClientShell'
import { ActivityTable } from './ProductActivity'
const tabs = ['Overview', 'Applications', 'Configuration', 'Identity', 'Domains', 'Activity', 'Secrets', 'Health'] as const
export function ClientWorkspace({ id, catalogue, onSaved }: { id: string; catalogue: Catalogue; onSaved: () => void }) {
  const [data, setData] = useState<ClientProductResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [editing, setEditing] = useState(false)
  const [preview, setPreview] = useState(false)
  const [tab, setTab] = useState<(typeof tabs)[number]>('Overview')
  const [generation, setGeneration] = useState(0)
  useEffect(() => {
    let active = true
    void getProduct(id).then((value) => { if (active) { setData(value); setError(null) } }, (error: unknown) => { if (active) setError(describe(error)) })
    return () => { active = false }
  }, [id, generation, tab])
  if (error) return <div className="error" role="alert">{error}<button onClick={() => { setGeneration((n) => n + 1) }}>Retry</button></div>
  if (!data) return <p role="status">Loading client…</p>
  if (editing) return <ClientForm catalogue={catalogue} existing={data} onSaved={() => { setEditing(false); setGeneration((n) => n + 1); onSaved() }} onCancel={() => { setEditing(false) }} />
  if (preview) return <ClientShell data={data} onBack={() => { setPreview(false) }} />
  return <><a className="back-link" href="#/clients">← All clients</a><PageHeader eyebrow="Client" title={data.client.displayName} actions={<button className="primary-button" onClick={() => { setEditing(true) }}>Configure client</button>} />
    <nav className="tabs" aria-label="Client sections">{tabs.map((name) => <button key={name} className={`tabs__tab${tab === name ? ' tabs__tab--current' : ''}`} aria-current={tab === name ? 'page' : undefined} onClick={() => { setTab(name) }}>{name}</button>)}</nav>
    {tab === 'Overview' && <><div className="metrics"><div className="metric"><span>Applications</span><strong>{data.product.applications.length}</strong></div><div className="metric"><span>Domains</span><strong>{data.client.hosts.length}</strong></div><div className="metric"><span>Identity</span><p><Status value={data.reconciliation.status} /></p></div></div>
      <Panel title="Client details"><div className="panel-body"><ClientFacts data={data} /><button onClick={() => { setPreview(true) }}>Preview client shell →</button></div></Panel></>}
    {tab === 'Applications' && <>{data.product.applications.length === 0 && <p>No applications assigned. Configure the client to add a published application.</p>}{data.product.applications.map((assignment) => <Panel key={assignment.applicationId} title={assignment.release.definition.name} action={<Status value="neutral">Definition v{assignment.release.version}</Status>}>
      <div className="panel-body"><p>{assignment.release.definition.plans.find((p) => p.id === assignment.planId)?.name} plan</p><p>Components: {data.resolved.find((r) => r.applicationId === assignment.applicationId)?.components.map((c) => c.name).join(', ') || 'None'}</p>
        <dl className="facts">{Object.entries(assignment.configuration).map(([key, value]) => <div className="fact-pair" key={key}><dt>{assignment.release.definition.fields.find((f) => f.key === key)?.label ?? key}</dt><dd>{value}</dd></div>)}</dl>
        <a href={`#/applications/${encodeURIComponent(assignment.applicationId)}`}>Open application definition →</a></div></Panel>)}</>}
    {tab === 'Configuration' && <Panel title={`Client definition v${String(data.product.definitionVersion)}`}><div className="panel-body"><ClientFacts data={data} />
      <dl className="facts">{Object.entries(data.product.configuration).map(([key, value]) => <div className="fact-pair" key={key}><dt>{catalogue.clientFields.find((f) => f.key === key)?.label ?? key}</dt><dd>{value}</dd></div>)}</dl></div></Panel>}
    {tab === 'Identity' && <IdentityPanel client={data.client} />}
    {tab === 'Secrets' && <Secrets client={data.client} />}
    {tab === 'Domains' && <Panel title="Declared domains"><div className="panel-body"><ul className="domain-list">{data.client.hosts.map((host) => <li key={host}><span className="mono">{host}</span><Status value="neutral">Declared</Status></li>)}</ul>
      <p>Manage hostnames through Configure client. Routing and certificate health require observations from the deployment system.</p></div></Panel>}
    {tab === 'Activity' && <ActivityTable activity={data.product.activity} />}
    {tab === 'Health' && <Panel title="Reconciliation status"><div className="panel-body"><p>Identity <Status value={data.reconciliation.status} /></p><p>{data.reconciliation.detail}</p>
      {data.resolved.flatMap((r) => r.components).map((component, index) => <p key={`${component.id}-${String(index)}`}>{component.name} <Status value="neutral">Deployment not observed</Status></p>)}<a href="#/reconciliation">Check reconciliation →</a></div></Panel>}
  </>
}
function ClientFacts({ data }: { data: ClientProductResponse }) {
  return <dl className="facts"><dt>Client ID</dt><dd>{data.client.id}</dd><dt>Legal name</dt><dd>{data.product.legalName || 'Not set'}</dd><dt>Region</dt><dd>{data.product.region || 'Not set'}</dd><dt>Timezone</dt><dd>{data.product.timezone || 'Not set'}</dd><dt>Identity realm</dt><dd>{data.client.realm}</dd></dl>
}
