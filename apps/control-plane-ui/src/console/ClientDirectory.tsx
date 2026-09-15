import { useState } from 'react'

import type { Client } from '../api/types'
import { ClientDirectoryRow } from './ClientDirectoryRow'
import { EmptyState } from './EmptyState'
import { PageHeader } from './PageHeader'
import type { Inventory } from './useInventory'

/**
 * Every client: searchable, filterable by identity status, and sortable.
 *
 * `inventory` is a shared read (see `useInventory`), not this component's
 * own fetch — the dashboard, this directory, and reconciliation all show the
 * same identity observations, and reading them three times would triple the
 * request count for no benefit an operator could see.
 *
 * This file sits in file-size-policy.md's 121-150 line band: the search,
 * filter and sort state, the filtering pipeline that reads it, and the
 * table it produces are one screen's worth of one concept — finding a
 * client — and splitting the toolbar from the table it filters would
 * separate two things that only make sense read together. The row itself
 * is already its own file, `ClientDirectoryRow`.
 */
export function ClientDirectory({
  clients,
  inventory,
}: {
  clients: readonly Client[]
  inventory: Inventory
}) {
  const [search, setSearch] = useState('')
  const [status, setStatus] = useState('all')
  const [sort, setSort] = useState('name')

  const entries = new Map(inventory.entries.map((entry) => [entry.client.id, entry]))
  const filtered = clients
    .filter((client) => {
      const match = `${client.displayName} ${client.id} ${client.hosts.join(' ')}`
        .toLowerCase()
        .includes(search.toLowerCase())
      return (
        match &&
        (status === 'all' || entries.get(client.id)?.identity?.reconciliation.status === status)
      )
    })
    .sort((a, b) =>
      sort === 'id' ? a.id.localeCompare(b.id) : a.displayName.localeCompare(b.displayName),
    )

  return (
    <>
      <PageHeader
        title="Clients"
        description="Every client, their domains, and the state of their identity configuration."
        actions={
          <a className="primary-link" href="#/create-client">
            + New client
          </a>
        }
      />

      <div className="toolbar">
        <label className="search">
          <span aria-hidden="true">⌕</span>
          <input
            aria-label="Search clients"
            placeholder="Search clients…"
            value={search}
            onChange={(event) => {
              setSearch(event.target.value)
            }}
          />
        </label>
        <select
          aria-label="Filter by status"
          value={status}
          onChange={(event) => {
            setStatus(event.target.value)
          }}
        >
          <option value="all">All statuses</option>
          {['applied', 'pending', 'failed', 'drifted'].map((value) => (
            <option key={value}>{value}</option>
          ))}
        </select>
        <select
          aria-label="Sort clients"
          value={sort}
          onChange={(event) => {
            setSort(event.target.value)
          }}
        >
          <option value="name">Name A–Z</option>
          <option value="id">Client ID A–Z</option>
        </select>
      </div>

      {filtered.length === 0 ? (
        <EmptyState title={clients.length === 0 ? 'No clients yet' : 'No matching clients'}>
          <p>
            {clients.length === 0
              ? 'Create your first client to configure its applications and identity.'
              : 'Try another name, domain, or status.'}
          </p>
          {clients.length > 0 && (
            <button
              onClick={() => {
                setSearch('')
                setStatus('all')
              }}
            >
              Clear filters
            </button>
          )}
        </EmptyState>
      ) : (
        <div className="table-wrap">
          <table className="fabric-table">
            <thead>
              <tr>
                <th>Client</th>
                <th>Domains</th>
                <th>Realm</th>
                <th>Identity status</th>
                <th>
                  <span className="sr-only">Open</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((client) => (
                <ClientDirectoryRow
                  key={client.id}
                  client={client}
                  entry={entries.get(client.id)}
                  loading={inventory.loading}
                />
              ))}
            </tbody>
          </table>
          <footer className="table-footer">
            {filtered.length} of {clients.length} clients
          </footer>
        </div>
      )}
    </>
  )
}
