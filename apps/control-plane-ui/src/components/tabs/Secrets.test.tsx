/**
 * What the Secrets tab puts on screen, and what it leaves behind.
 *
 * The persistence assertions are the load-bearing ones. A revealed value is
 * the one thing this console handles that must not outlive the tab, and
 * nothing about `localStorage` fails loudly when it is used by accident.
 */
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Client } from '../../api/types'
import { Secrets } from './Secrets'

const ACME: Client = {
  id: 'acme',
  displayName: 'Acme',
  hosts: [],
  realm: 'acme',
  revision: 'abc',
}

const VALUE = 'a-value-that-must-not-persist'

/** Answers the console's calls without a control plane. */
function api(): ReturnType<typeof vi.fn> {
  const fetched = vi.fn((path: string, init?: { method?: string }) => {
    if (path.endsWith('/secrets') && (init?.method ?? 'GET') === 'GET') {
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve([{ path: 'database/primary' }]),
      })
    }

    if (path.endsWith('/secrets/reveal')) {
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ values: { password: VALUE } }),
      })
    }

    return Promise.resolve({ ok: true, status: 204, json: () => Promise.resolve({}) })
  })

  vi.stubGlobal('fetch', fetched)

  return fetched
}

beforeEach(() => {
  localStorage.clear()
  sessionStorage.clear()
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the secrets tab', () => {
  it('lists paths without showing any value', async () => {
    api()
    render(<Secrets client={ACME} />)

    await waitFor(() => {
      expect(screen.getByText('database/primary')).toBeDefined()
    })

    // The listing never carries values, so there is nothing to hide badly.
    expect(screen.getByText('hidden')).toBeDefined()
    expect(screen.queryByText(VALUE)).toBeNull()
  })

  it('shows a value only after somebody asks for that secret', async () => {
    api()
    render(<Secrets client={ACME} />)

    await waitFor(() => {
      expect(screen.getByText('database/primary')).toBeDefined()
    })

    expect(screen.queryByText(VALUE)).toBeNull()

    await userEvent.click(screen.getByRole('button', { name: 'Reveal' }))

    await waitFor(() => {
      expect(screen.getByText(VALUE)).toBeDefined()
    })
  })

  it('never persists a revealed value anywhere the browser keeps things', async () => {
    api()
    render(<Secrets client={ACME} />)

    await waitFor(() => {
      expect(screen.getByText('database/primary')).toBeDefined()
    })

    await userEvent.click(screen.getByRole('button', { name: 'Reveal' }))
    await waitFor(() => {
      expect(screen.getByText(VALUE)).toBeDefined()
    })

    // On screen, and nowhere the browser keeps things. The response says
    // `no-store`; a copy here would defeat that at the last step.
    //
    // Read key by key rather than spread: `Storage` is a class instance, and
    // spreading one loses its prototype — the same trap that caught an earlier
    // test in this repository.
    const stored = [localStorage, sessionStorage].flatMap((store) =>
      Array.from({ length: store.length }, (_, index) => {
        const key = store.key(index) ?? ''

        return [key, store.getItem(key) ?? ''] as const
      }),
    )

    expect(stored.length).toBe(0)
    expect(JSON.stringify(stored)).not.toContain(VALUE)
  })

  it('forgets a revealed value when it is hidden again', async () => {
    api()
    render(<Secrets client={ACME} />)

    await waitFor(() => {
      expect(screen.getByText('database/primary')).toBeDefined()
    })

    await userEvent.click(screen.getByRole('button', { name: 'Reveal' }))
    await waitFor(() => {
      expect(screen.getByText(VALUE)).toBeDefined()
    })

    await userEvent.click(screen.getByRole('button', { name: 'Hide' }))

    expect(screen.queryByText(VALUE)).toBeNull()
    expect(screen.getByText('hidden')).toBeDefined()
  })

  it('reveals by POST rather than by putting the path in a URL', async () => {
    const fetched = api()
    render(<Secrets client={ACME} />)

    await waitFor(() => {
      expect(screen.getByText('database/primary')).toBeDefined()
    })

    await userEvent.click(screen.getByRole('button', { name: 'Reveal' }))
    await waitFor(() => {
      expect(screen.getByText(VALUE)).toBeDefined()
    })

    const reveal = fetched.mock.calls.find(([path]) => String(path).endsWith('/secrets/reveal'))

    expect(reveal).toBeDefined()
    // A URL would carry the path into history, referrers and proxy logs.
    expect((reveal?.[1] as { method?: string } | undefined)?.method).toBe('POST')
    expect(String(reveal?.[0])).not.toContain('database')
  })
})
