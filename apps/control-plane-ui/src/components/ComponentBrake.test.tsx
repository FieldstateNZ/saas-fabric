/**
 * The brake, from an operator's side.
 *
 * What these pin is the distinction the whole control turns on: pausing stops
 * an environment moving and does not move it, and it is not a policy change.
 */
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { PlatformComponent } from '../api/types'
import { ComponentBrake } from './ComponentBrake'

function component(overrides: Partial<PlatformComponent> = {}): PlatformComponent {
  return {
    component: 'saas-fabric',
    desired: '0.3.0-preview.3',
    newer: null,
    running: 'unknown',
    policy: 'automatic',
    artifact: 'oci',
    paused: false,
    desiredState: 'current',
    hold: null,
    diagnostics: [],
    ...overrides,
  }
}

function accepted() {
  const fetched = vi.fn().mockResolvedValue({
    ok: true,
    status: 200,
    json: () => Promise.resolve({}),
  })
  vi.stubGlobal('fetch', fetched)
  vi.stubGlobal('location', { reload: vi.fn() })

  return fetched
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('the brake', () => {
  it('offers rollback, and not pause, for a component that does not advance on its own', () => {
    // There is no advancement to stop, and the control plane refuses a pause
    // for it -- a button whose only outcome is a refusal should not exist. But
    // the component can still be put back on a version it ran before, and a
    // stable chart on `manual` is exactly the one an operator will need that
    // for, since `manual` is what the platform recommends for it.
    for (const policy of ['manual', 'locked'] as const) {
      const { unmount } = render(<ComponentBrake component={component({ policy, artifact: 'helm' })} />)

      expect(screen.getByRole('button', { name: 'Roll back' })).toBeInTheDocument()
      expect(
        screen.queryByRole('button', { name: /pause automatic updates/i }),
      ).not.toBeInTheDocument()

      unmount()
    }
  })

  it('pauses through the hold, carrying the note and never a version', async () => {
    const fetched = accepted()

    render(<ComponentBrake component={component()} />)
    await userEvent.click(screen.getByRole('button', { name: /pause automatic updates/i }))
    await userEvent.type(screen.getByLabelText(/why/i), 'testing preview.4 by hand')
    await userEvent.click(screen.getByRole('button', { name: 'Pause' }))

    const [url, options] = fetched.mock.calls[0] as [string, { method: string; body?: string }]

    expect(url).toBe('/api/platform/components/saas-fabric/hold')
    expect(options.method).toBe('PUT')

    const body = JSON.parse(options.body ?? '{}') as Record<string, unknown>

    expect(body).toEqual({ note: 'testing preview.4 by hand' })
    // Not a version, and not a policy. Pausing stops an environment moving; it
    // does not move it, and it does not decide it should stop forever.
    expect(Object.keys(body)).toEqual(['note'])
  })

  it('offers resume and rollback, and not pause, once it is paused', async () => {
    const fetched = accepted()

    render(<ComponentBrake component={component({ paused: true })} />)

    expect(
      screen.queryByRole('button', { name: /pause automatic updates/i }),
    ).not.toBeInTheDocument()
    // An operator who paused because a release broke is the one who needs to
    // go back; hiding rollback behind the pause would send them to the
    // repository by hand.
    expect(screen.getByRole('button', { name: 'Roll back' })).toBeInTheDocument()

    await userEvent.click(screen.getByRole('button', { name: /resume automatic updates/i }))

    const [url, options] = fetched.mock.calls[0] as [string, { method: string; body?: string }]

    expect(url).toBe('/api/platform/components/saas-fabric/hold')
    expect(options.method).toBe('DELETE')
  })

  it('shows what went wrong rather than pretending it worked', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 409,
        json: () =>
          Promise.resolve({
            error: {
              code: 'component_not_advancing',
              message: 'saas-fabric does not advance on its own.',
            },
          }),
      }),
    )

    render(<ComponentBrake component={component()} />)
    await userEvent.click(screen.getByRole('button', { name: /pause automatic updates/i }))
    await userEvent.click(screen.getByRole('button', { name: 'Pause' }))

    expect(await screen.findByText(/does not advance on its own/)).toBeInTheDocument()
  })
})

describe('rolling back, for either artifact kind', () => {
  it('offers rollback for a chart and says what it restores', async () => {
    // Rolling back means restoring a previously selected desired version, and
    // a chart supports that as much as an image does. What it does not restore
    // is the bytes — a chart repository can republish what sits behind a
    // version — so the operator is told, rather than the button being absent
    // and the capability with it.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ versions: [{ version: '7.3.0' }], more: false }),
      }),
    )
    vi.stubGlobal('location', { reload: vi.fn() })

    render(<ComponentBrake component={component({ artifact: 'helm' })} />)

    await userEvent.click(screen.getByRole('button', { name: 'Roll back' }))

    expect(
      await screen.findByText(/a chart repository can republish the bytes behind a version/i),
    ).toBeInTheDocument()
    expect(await screen.findByRole('button', { name: '7.3.0' })).toBeInTheDocument()
  })

  it('says nothing about bytes for a component published as images', async () => {
    // The caveat is true of one kind, so it is shown for one kind. A console
    // that hedged about every rollback would be teaching an operator to ignore
    // the line that matters.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: () =>
          Promise.resolve({
            versions: [{ version: '0.3.0-preview.2', source_revision: 'b08ba9e' }],
            more: false,
          }),
      }),
    )
    vi.stubGlobal('location', { reload: vi.fn() })

    render(<ComponentBrake component={component()} />)

    await userEvent.click(screen.getByRole('button', { name: 'Roll back' }))
    await screen.findByRole('button', { name: '0.3.0-preview.2' })

    expect(screen.queryByText(/republish the bytes/i)).not.toBeInTheDocument()
  })
})
