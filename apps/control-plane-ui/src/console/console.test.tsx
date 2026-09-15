import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Client, Identity } from '../api/types'
import { ClientDirectory } from './ClientDirectory'
import { Dashboard } from './Dashboard'
import { clientHref, readRoute } from './navigation'
import { Reconciliation } from './Reconciliation'
import type { Inventory } from './useInventory'
import { converge } from '../api/client'

vi.mock('../api/client', () => ({ converge: vi.fn() }))
const clients: Client[] = [
  { id: 'acme', displayName: 'Acme', hosts: ['acme.example.test'], realm: 'acme', revision: 'r1' },
  { id: 'northstar', displayName: 'Northstar', hosts: ['north.example.test'], realm: 'northstar', revision: 'r1' },
]
const identity: Identity = { realm: 'acme', roles: [], clients: [], apiVersion: 'v2', revision: 'r1',
  reconciliation: { status: 'pending', observedAtUnix: null, detail: null } }
const inventory: Inventory = { loading: false, refresh: vi.fn(), entries: clients.map((client) => ({ client, identity, error: null })) }
afterEach(() => { window.history.replaceState({}, '', '/'); vi.clearAllMocks() })

describe('phase-one console', () => {
  it('searches domains and clears an empty result', async () => {
    render(<ClientDirectory clients={clients} inventory={inventory} />)
    await userEvent.type(screen.getByRole('textbox', { name: 'Search clients' }), 'north.example')
    expect(screen.queryByRole('link', { name: 'Open Acme' })).toBeNull()
    expect(screen.getByRole('link', { name: 'Open Northstar' })).toHaveAttribute('href', '#/clients/northstar')
    await userEvent.selectOptions(screen.getByLabelText('Filter by status'), 'failed')
    expect(screen.getByRole('heading', { name: 'No matching clients' })).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Clear filters' }))
    expect(screen.getByRole('link', { name: 'Open Acme' })).toBeInTheDocument()
  })
  it('never renders incomplete identity totals as zero or platform health as healthy', () => {
    const incomplete = { ...inventory, entries: clients.map((client) => ({ client, identity: null, error: 'Access denied' })) }
    render(<Dashboard clients={clients} platform={null} inventory={incomplete} />)
    expect(within(screen.getByRole('link', { name: /Pending identities/ })).getByText('—')).toBeInTheDocument()
    expect(screen.getByText('Attention required')).toBeInTheDocument()
    expect(screen.queryByText('Healthy')).toBeNull()
  })
  it('refreshes observations only after a successful convergence request', async () => {
    vi.mocked(converge).mockResolvedValue({ clients: 2 })
    render(<Reconciliation inventory={inventory} connected />)
    await userEvent.click(screen.getByRole('button', { name: 'Check for drift' }))
    expect(await screen.findByText('Checked 2 clients.')).toBeInTheDocument()
    expect(inventory.refresh).toHaveBeenCalledOnce()
  })
  it('retains deep links and falls back safely for malformed hashes', () => {
    window.history.replaceState({}, '', clientHref('a b'))
    expect(readRoute()).toEqual({ page: 'clients', clientId: 'a b' })
    window.history.replaceState({}, '', '#/clients/%E0%A4%A')
    expect(readRoute()).toEqual({ page: 'clients', clientId: null })
    window.history.replaceState({}, '', '#/does-not-exist')
    expect(readRoute().page).toBe('overview')
  })
  it('lands integration callbacks on Integrations', () => {
    window.history.replaceState({}, '', '/?platform=connected#/clients')
    expect(readRoute().page).toBe('integrations')
  })
})
