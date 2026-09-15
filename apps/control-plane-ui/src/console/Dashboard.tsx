import type { Catalogue } from '../api/catalogue-types'
import type { Client, Platform } from '../api/types'
import type { Inventory } from './useInventory'
import { Activity } from './Activity'
import { clientHref } from './navigation'
import { PageHeader, Panel, Status } from './primitives'

export function Dashboard({ clients, platform, inventory, catalogue }: {
  clients: readonly Client[] | null; platform: Platform | null; inventory: Inventory; catalogue?: Catalogue | undefined
}) {
  const known = !inventory.loading && inventory.entries.every((entry) => entry.identity !== null)
  const pending = inventory.entries.filter((entry) => entry.identity?.reconciliation.status === 'pending').length
  const applications = new Set(inventory.entries.flatMap((entry) => entry.identity?.clients.map((app) => app.id) ?? []))
  const attention = inventory.entries.filter((entry) => entry.error !== null || ['failed', 'drifted'].includes(entry.identity?.reconciliation.status ?? ''))
  const metrics = [
    { label: 'Clients', value: clients?.length, to: 'clients' },
    { label: catalogue ? 'Applications' : 'Sign-in applications', value: catalogue?.applications.length ?? (known && clients !== null ? applications.size : undefined), to: 'applications' },
    { label: 'Components', value: platform?.components.length, to: 'components' },
    { label: 'Pending identities', value: known && clients !== null ? pending : undefined, to: 'reconciliation' },
  ]
  return <>
    <PageHeader title={catalogue?.settings.platformName ?? platform?.environment ?? 'SaaS Fabric'} actions={<Status value="neutral">Operator console</Status>} />
    <div className="metrics">{metrics.map((metric) => <a className="metric" key={metric.to} href={`#/${metric.to}`}>
      <span>{metric.label}</span><strong>{metric.value ?? '—'}</strong><span className="metric-arrow" aria-hidden="true">↗</span></a>)}</div>
    {attention.length > 0 && !inventory.loading && <section className="attention"><p className="eyebrow">Attention required</p>
      {attention.map(({ client, error, identity }) => <div key={client.id}><p><strong>{client.displayName}</strong> — {error ?? identity?.reconciliation.detail ?? 'Identity configuration needs attention.'}</p>
        <a href={clientHref(client.id)}>Open →</a></div>)}</section>}
    <Activity inventory={inventory} limit={6} />
    <div className="dashboard-bottom"><Panel title="Your platform"><div className="panel-body"><p>One place to manage client identity, configuration, and platform updates.</p>
      <a href="#/clients">Explore clients →</a></div></Panel>
      <Panel title="Platform management"><div className="panel-body"><p>{platform ? `Managing ${platform.environment}. Review desired versions and update policies.` : 'Connect platform management to inspect components and update policies.'}</p>
        <a href={platform ? '#/components' : '#/integrations'}>{platform ? 'View components →' : 'Manage integrations →'}</a></div></Panel></div>
  </>
}
