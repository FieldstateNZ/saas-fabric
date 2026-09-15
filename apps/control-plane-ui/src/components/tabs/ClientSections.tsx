import type { Client } from '../../api/types'
import { useIdentity } from '../../hooks/useIdentity'
import { ApplicationClients } from '../ApplicationClients'
import { ReconciliationBadge } from '../ReconciliationBadge'

/** Additional phase-one views use only facts already exposed by the client API. */
export function ClientApplications({ client }: { client: Client }) {
  const identity = useIdentity(client.id)
  if (identity.loading) return <p role="status">Loading applications…</p>
  if (identity.error) return <p className="error" role="alert">{identity.error}</p>
  return <section className="overview"><h2>Sign-in applications</h2>
    {identity.value && <ApplicationClients clients={identity.value.clients} apiVersion={identity.value.apiVersion} />}
    <p className="support-note">Application plans and feature assignments are not available yet.</p></section>
}
export function ClientDomains({ client }: { client: Client }) {
  return <section className="overview"><h2>Domains</h2>
    {client.hosts.length === 0 ? <p className="empty">No domains configured.</p> : <ul className="domain-list">
      {client.hosts.map((host) => <li key={host}><span className="mono">{host}</span><span className="type-tag">Declared hostname</span></li>)}
    </ul>}<p className="support-note">Domain verification, routing health, and domain editing are not available yet.</p></section>
}
export function ClientConfiguration({ client }: { client: Client }) {
  return <section className="overview"><h2>Client configuration</h2><dl className="overview__fields">
    <dt>Client ID</dt><dd className="mono">{client.id}</dd><dt>Display name</dt><dd>{client.displayName}</dd>
    <dt>Domains</dt><dd>{client.hosts.join(', ') || 'None'}</dd><dt>Identity realm</dt><dd className="mono">{client.realm}</dd>
  </dl><p className="support-note">This core configuration is read only. Manage roles in Identity and values in Secrets.</p></section>
}
export function ClientActivity({ client }: { client: Client }) {
  const identity = useIdentity(client.id)
  return <section className="overview"><h2>Latest identity observation</h2>
    {identity.loading && <p role="status">Loading observation…</p>}
    {identity.error && <p className="error" role="alert">{identity.error}</p>}
    {identity.value && <ReconciliationBadge reconciliation={identity.value.reconciliation} />}
    <p className="support-note">A full client activity history is not available yet.</p></section>
}
