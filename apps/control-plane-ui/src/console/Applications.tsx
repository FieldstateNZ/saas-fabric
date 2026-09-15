import { useState } from 'react'
import type { Inventory } from './useInventory'
import { clientHref } from './navigation'
import { EmptyState, PageHeader, Panel, Status } from './primitives'

export function Applications({ inventory }: { inventory: Inventory }) {
  const [search, setSearch] = useState('')
  const ids = [...new Set(inventory.entries.flatMap((entry) => entry.identity?.clients.map((app) => app.id) ?? []))].sort()
  const filtered = ids.filter((id) => id.toLowerCase().includes(search.toLowerCase()))
  return <><PageHeader title="Applications" description="Sign-in applications configured for your clients." />
    <div className="toolbar"><label className="search"><span aria-hidden="true">⌕</span><input aria-label="Search applications" placeholder="Search applications…"
      value={search} onChange={(event) => { setSearch(event.target.value) }} /></label></div>
    <p className="support-note">Application definitions, plans, features, and releases are not available in this deployment yet.</p>
    {inventory.loading ? <p role="status">Loading applications…</p> : <>
      {inventory.entries.some((entry) => entry.error !== null) && <p className="error" role="alert">Some clients could not be loaded. This application list may be incomplete.</p>}
      {filtered.length === 0 ? <EmptyState title="No applications found"><p>{search ? 'Try a different application name.' : 'Applications appear here when a client has sign-in configuration.'}</p></EmptyState>
        : <div className="application-grid">{filtered.map((id) => {
          const owners = inventory.entries.filter((entry) => entry.identity?.clients.some((app) => app.id === id))
          return <Panel key={id} title={id} action={<Status value="neutral">OIDC</Status>}><div className="panel-body">
            <p>{owners.length} client{owners.length === 1 ? '' : 's'} · PKCE S256</p>
            <ul className="application-owners">{owners.map(({ client, identity }) => <li key={client.id}>
              <a href={clientHref(client.id)}>{client.displayName} →</a>
              <small className="mono">{identity?.clients.find((app) => app.id === id)?.redirect.uris.join(', ') || 'No redirect URIs'}</small>
            </li>)}</ul></div></Panel>
        })}</div>}
    </>}</>
}
