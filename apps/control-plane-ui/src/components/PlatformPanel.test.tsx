import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { Platform, PlatformComponent } from '../api/types'
import { PlatformNotManaged, PlatformPanel } from './PlatformPanel'

/** A component as the API reports it. */
function component(overrides: Partial<PlatformComponent> = {}): PlatformComponent {
  return {
    component: 'saas-fabric',
    desired: 'v0.3.0-preview.2',
    newer: 'v0.3.0-preview.3',
    running: 'unknown',
    policy: 'automatic',
    artifact: 'oci',
    paused: false,
    desiredState: 'update-available',
    hold: null,
    diagnostics: [],
    ...overrides,
  }
}

/** An environment as the API reports it. */
function platform(overrides: Partial<Platform> = {}): Platform {
  return {
    environment: 'lucentroot',
    components: [component()],
    lastCheck: { atUnixSeconds: 1_700_000_000, outcome: 'success', detail: null },
    ...overrides,
  }
}

describe('the platform panel', () => {
  it('shows what is desired, what it would advance to, and that running is unknown', () => {
    render(<PlatformPanel platform={platform()} />)

    expect(screen.getByText('v0.3.0-preview.2')).toBeInTheDocument()
    expect(screen.getByText('v0.3.0-preview.3')).toBeInTheDocument()
    // Not blank, and not a guess. Git having changed is not a rollout.
    expect(screen.getByText('Unknown')).toBeInTheDocument()
    expect(screen.getByText('Update available')).toBeInTheDocument()
  })

  it('says never when nothing has checked yet', () => {
    // The distinction the whole capability turns on: an operator whose
    // published version has not appeared must be able to tell "nothing has
    // looked" from "something looked and found nothing".
    render(<PlatformPanel platform={platform({ lastCheck: null })} />)

    expect(screen.getByText('Never')).toBeInTheDocument()
  })

  it('shows when the last check succeeded', () => {
    render(<PlatformPanel platform={platform()} />)

    expect(screen.getByText(/— success$/)).toBeInTheDocument()
  })

  it('shows why the last check failed', () => {
    render(
      <PlatformPanel
        platform={platform({
          lastCheck: {
            atUnixSeconds: 1_700_000_000,
            outcome: 'failure',
            detail: 'saas-fabric: the registry is unavailable',
          },
        })}
      />,
    )

    expect(screen.getByText(/registry is unavailable/)).toBeInTheDocument()
  })

  it('reads a hold as paused without claiming the policy changed', () => {
    render(
      <PlatformPanel
        platform={platform({
          components: [
            component({
              paused: true,
              hold: { reason: 'rollback', since: '2026-09-01T09:00:00Z', note: 'preview.7 broke Secrets' },
            }),
          ],
        })}
      />,
    )

    // Still automatic. The operator paused advancement; they did not decide
    // this environment should stop being automatic.
    expect(screen.getByText('Automatic — Paused')).toBeInTheDocument()
    expect(screen.getByText(/preview.7 broke Secrets/)).toBeInTheDocument()
  })

  it('distinguishes a version still publishing from one built more than once', () => {
    render(
      <PlatformPanel
        platform={platform({
          components: [
            component({
              diagnostics: [
                { version: 'v0.3.0-preview.4', state: 'publishing' },
                { version: 'v0.3.0-preview.3', state: 'incoherent' },
              ],
            }),
          ],
        })}
      />,
    )

    // One will fix itself and one will not, and an operator deciding whether
    // to wait needs to know which.
    expect(screen.getByText(/still publishing/)).toBeInTheDocument()
    expect(screen.getByText(/built more than once/)).toBeInTheDocument()
  })

  it('shows a dash rather than a version when nothing newer exists', () => {
    render(
      <PlatformPanel
        platform={platform({ components: [component({ newer: null, desiredState: 'current' })] })}
      />,
    )

    expect(screen.getByText('Current')).toBeInTheDocument()
    expect(screen.getByText('—')).toBeInTheDocument()
  })

  it('does not call the dash "Available", because the desired version is', () => {
    // The wart this row was renamed for. An environment on the newest preview
    // rendered `Available —`, which reads as "nothing is available" about a
    // version that plainly is. The narrower fact gets the narrower label:
    // there is nothing to advance *to*.
    render(
      <PlatformPanel
        platform={platform({ components: [component({ newer: null, desiredState: 'current' })] })}
      />,
    )

    expect(screen.getByText('Newer version')).toBeInTheDocument()
    expect(screen.queryByText('Available')).not.toBeInTheDocument()
  })
})

describe('a deployment that manages no platform', () => {
  it('says so plainly rather than reporting an error', () => {
    // It is a state an operator can act on, not a fault. Styling it as an
    // error would send them looking for something broken.
    render(<PlatformNotManaged />)

    expect(screen.getByText(/not connected for this deployment/i)).toBeInTheDocument()
  })
})
