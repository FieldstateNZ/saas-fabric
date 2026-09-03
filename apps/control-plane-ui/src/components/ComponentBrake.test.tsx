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
    rollable: true,
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
  it('offers nothing for a component that does not advance on its own', () => {
    // There is no advancement to stop, and the control plane refuses it. A
    // button whose only outcome is a refusal is a button that should not exist.
    const { container } = render(<ComponentBrake component={component({ policy: 'manual' })} />)

    expect(container).toBeEmptyDOMElement()
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

  it('offers resume, and only resume, once it is paused', async () => {
    const fetched = accepted()

    render(<ComponentBrake component={component({ paused: true })} />)

    expect(
      screen.queryByRole('button', { name: /pause automatic updates/i }),
    ).not.toBeInTheDocument()

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

describe('a component whose versions are not immutable', () => {
  it('is not offered a rollback whose only outcome is a refusal', () => {
    // A chart repository pins a version, not a digest — the bytes behind it
    // can be republished, so "put me back on what I was running" is a promise
    // the platform cannot keep. The control plane refuses it; the console does
    // not offer it.
    render(<ComponentBrake component={component({ rollable: false })} />)

    expect(screen.queryByRole('button', { name: 'Roll back' })).not.toBeInTheDocument()

    // Pausing is still offered: stopping an environment advancing needs no
    // immutability at all.
    expect(
      screen.getByRole('button', { name: /pause automatic updates/i }),
    ).toBeInTheDocument()
  })
})
