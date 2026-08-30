/**
 * What the client detail view shows, including what it cannot.
 *
 * The assertions about *missing* things are the load-bearing ones. A tab that
 * quietly disappears until its API arrives leaves an operator unable to tell
 * "SaaS Fabric does not do this" from "SaaS Fabric does this and I cannot see
 * it", and those need very different conversations.
 */
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import type { Client } from '../api/types'
import { ClientDetail } from './ClientDetail'
import { CLIENT_TABS } from './tabs/tabs'

const ACME: Client = {
  id: 'acme',
  displayName: 'Acme',
  hosts: ['www.acme.example'],
  realm: 'acme',
  revision: 'abc123',
}

describe('the client detail view', () => {
  it('offers every section of the product surface', () => {
    render(<ClientDetail client={ACME} />)

    for (const tab of CLIENT_TABS) {
      expect(screen.getByRole('button', { name: tab })).toBeDefined()
    }
  })

  it('opens on what is actually known about the client', () => {
    render(<ClientDetail client={ACME} />)

    expect(screen.getByText('acme', { selector: 'dd' })).toBeDefined()
    expect(screen.getByText('www.acme.example')).toBeDefined()
  })

  it('names the fields it cannot show rather than dropping them', () => {
    render(<ClientDetail client={ACME} />)

    // Every intended field appears, whether or not there is an API behind it.
    // Scoped to the field list, because some names are also tab names — which
    // is itself the point: the tab promises a section, the field says the
    // section has nothing behind it yet.
    for (const field of ['Issuer', 'Secret partition', 'Authorization store', 'Modules']) {
      expect(screen.getByText(field, { selector: 'dt' })).toBeDefined()
    }

    expect(screen.getAllByText(/Not exposed yet/).length).toBeGreaterThan(0)
  })

  it('says what an unbuilt tab will show and what it needs', async () => {
    render(<ClientDetail client={ACME} />)

    await userEvent.click(screen.getByRole('button', { name: 'Secrets' }))

    expect(screen.getByText(/paths, keys, and values revealed one at a time/)).toBeDefined()
    // The requirement, stated on the screen rather than guessed at in a backlog.
    expect(screen.getByText(/No per-client secret API exists/)).toBeDefined()
  })

  it('keeps the tabs usable by naming the current one', async () => {
    render(<ClientDetail client={ACME} />)

    expect(screen.getByRole('button', { name: 'Overview' }).getAttribute('aria-current')).toBe('page')

    await userEvent.click(screen.getByRole('button', { name: 'Health' }))

    expect(screen.getByRole('button', { name: 'Health' }).getAttribute('aria-current')).toBe('page')
    expect(screen.getByRole('button', { name: 'Overview' }).getAttribute('aria-current')).toBeNull()
  })
})
