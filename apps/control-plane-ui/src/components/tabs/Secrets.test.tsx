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

/** A synthetic value for the write form. Not a real secret. */
const NEW_VALUE = 'synthetic-test-value-not-a-secret'

/** The `expectedVersion` the console actually serialised in its one PUT, or `undefined` if it never wrote. */
function sentExpectedVersion(fetched: ReturnType<typeof vi.fn>): unknown {
  const puts = fetched.mock.calls.filter(
    ([, init]) => (init as { method?: string } | undefined)?.method === 'PUT',
  )

  expect(puts.length).toBe(1)

  const body = JSON.parse(String((puts[0]?.[1] as { body?: string } | undefined)?.body)) as {
    expectedVersion?: unknown
  }

  // The exact serialised key, not a coerced reading of it: a `null` here is
  // "create only", and that is the whole bug.
  expect(Object.hasOwn(body, 'expectedVersion')).toBe(true)

  return body.expectedVersion
}

/** Fills the write form the way an operator would, leaving the version as given. */
async function fillWriteForm(version: string): Promise<void> {
  await userEvent.type(screen.getByLabelText('Path'), 'database/replica')
  await userEvent.type(screen.getByLabelText('Key'), 'password')
  await userEvent.type(screen.getByLabelText('Value'), NEW_VALUE)

  if (version !== '') {
    await userEvent.type(screen.getByLabelText('Replacing version'), version)
  }
}

describe('writing a secret against a version', () => {
  async function ready(): Promise<ReturnType<typeof vi.fn>> {
    const fetched = api()
    render(<Secrets client={ACME} />)

    await waitFor(() => {
      expect(screen.getByText('database/primary')).toBeDefined()
    })

    return fetched
  }

  it.each([
    ['text', 'abc'],
    ['Infinity', 'Infinity'],
    ['a negative number', '-1'],
    ['a fraction', '1.5'],
    ['an integer past the safe range', '9007199254740993'],
    // `Number()` would round each of the next three to a safe integer before
    // `isSafeInteger` ever saw them: the first to the maximum, the second to 1,
    // and the third underflows to 0 — which the store reads as "create only".
    ['a fraction that rounds to the maximum', '9007199254740991.1'],
    ['a fraction that rounds to one', '1.0000000000000001'],
    ['an exponent that underflows to zero', '1e-999'],
    ['an exponent', '1e3'],
    ['hexadecimal', '0x10'],
    ['a signed number', '+3'],
  ])('refuses %s without sending anything, and keeps what was typed', async (_label, typed) => {
    const fetched = await ready()
    await fillWriteForm(typed)

    await userEvent.click(screen.getByRole('button', { name: 'Write' }))

    const alert = await screen.findByRole('alert')
    // The message has to say what is accepted — decimal digits and the range —
    // not merely that something was wrong.
    expect(alert.textContent).toMatch(/whole number/i)
    expect(alert.textContent).toMatch(/decimal digits/i)
    expect(alert.textContent).toContain('9007199254740991')

    const versionField = screen.getByLabelText('Replacing version')
    expect(versionField).toHaveAttribute('aria-invalid', 'true')
    expect(versionField).toHaveAccessibleDescription(alert.textContent)
    expect(versionField).toHaveFocus()

    // No write of any kind reached the API: not a create, not a replacement.
    expect(
      fetched.mock.calls.some(
        ([, init]) => (init as { method?: string } | undefined)?.method === 'PUT',
      ),
    ).toBe(false)

    // Everything stays as typed, so the operator fixes one field rather than
    // starting over.
    expect(screen.getByLabelText('Path')).toHaveValue('database/replica')
    expect(screen.getByLabelText('Key')).toHaveValue('password')
    expect(screen.getByLabelText('Value')).toHaveValue(NEW_VALUE)
    expect(versionField).toHaveValue(typed)
  })

  it('sends null for a blank version, meaning "this does not exist yet"', async () => {
    const fetched = await ready()
    await fillWriteForm('')
    await userEvent.type(screen.getByLabelText('Replacing version'), '   ')

    await userEvent.click(screen.getByRole('button', { name: 'Write' }))

    await waitFor(() => {
      expect(sentExpectedVersion(fetched)).toBeNull()
    })
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it.each([
    ['3', 3],
    // Zero is well-formed, and the store reads it as "create only" — the same
    // as blank — so the console sends it as typed rather than second-guessing.
    ['0', 0],
    ['007', 7],
    ['9007199254740991', Number.MAX_SAFE_INTEGER],
    ['  12  ', 12],
  ])('sends version %s exactly as the number %i', async (typed, expected) => {
    const fetched = await ready()
    await fillWriteForm(typed)

    await userEvent.click(screen.getByRole('button', { name: 'Write' }))

    await waitFor(() => {
      expect(sentExpectedVersion(fetched)).toBe(expected)
    })
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('writes once the version is corrected after a refusal', async () => {
    const fetched = await ready()
    await fillWriteForm('abc')

    await userEvent.click(screen.getByRole('button', { name: 'Write' }))
    await screen.findByRole('alert')

    const versionField = screen.getByLabelText('Replacing version')
    await userEvent.clear(versionField)
    await userEvent.type(versionField, '2')

    expect(screen.queryByRole('alert')).toBeNull()

    await userEvent.click(screen.getByRole('button', { name: 'Write' }))

    await waitFor(() => {
      expect(sentExpectedVersion(fetched)).toBe(2)
    })
    expect(await screen.findByText('Wrote database/replica.')).toBeDefined()
  })
})
