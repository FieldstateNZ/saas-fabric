/**
 * What the integrations panel says, and what it refuses to offer.
 *
 * The distinctions under test are the ones the control plane went to the
 * trouble of reporting separately. `Unavailable` is the one that matters: an
 * integration that exists and does not work must not read as one nobody has
 * connected, because an operator shown that connects it a second time instead
 * of finding out why the first one stopped.
 */
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { Application, Integration, PlatformIntegration } from '../api/types'
import type { PlatformState } from '../hooks/usePlatform'
import { IntegrationsPanel } from './IntegrationsPanel'

const CHOSEN: Application = {
  slug: 'saas-fabric-platform',
  account: 'FieldstateNZ',
  installed: true,
  repository: 'FieldstateNZ/saas-fabric-platform',
}

function clients(overrides: Partial<Integration> = {}): Integration {
  return {
    status: 'connected',
    connection: 'FieldstateNZ/saas-fabric-clients',
    last_success_at: null,
    managed: true,
    application: {
      slug: 'saas-fabric',
      account: 'FieldstateNZ',
      installed: true,
      repository: 'FieldstateNZ/saas-fabric-clients',
    },
    ...overrides,
  }
}

function platformApplication(overrides: Partial<PlatformIntegration> = {}): PlatformIntegration {
  return { managed: true, application: CHOSEN, ...overrides }
}

function platformState(overrides: Partial<PlatformState> = {}): PlatformState {
  return {
    value: { environment: 'lucentroot', components: [], lastCheck: null },
    loading: false,
    error: null,
    unmanaged: false,
    ...overrides,
  }
}

function panel(
  platform: PlatformState = platformState(),
  application: PlatformIntegration = platformApplication(),
  client: Integration = clients(),
) {
  return render(
    <IntegrationsPanel
      clients={client}
      platformApplication={application}
      platform={platform}
    />,
  )
}

/** The card for one integration, so assertions cannot land on the other. */
function card(name: string): HTMLElement {
  return screen.getByRole('heading', { name }).closest('section') as HTMLElement
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('the integrations panel', () => {
  it('shows the two integrations as two things, not a list of one kind', () => {
    panel()

    expect(screen.getByRole('heading', { name: 'Client Configuration' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Platform Management' })).toBeInTheDocument()
  })

  it('reports a bound platform repository as connected', () => {
    panel()

    expect(within(card('Platform Management')).getByText('Connected')).toBeInTheDocument()
    expect(
      within(card('Platform Management')).getByText('FieldstateNZ/saas-fabric-platform'),
    ).toBeInTheDocument()
  })

  it('calls a connected platform that cannot be read unavailable, not disconnected', () => {
    panel(platformState({ value: null, error: 'The platform repository timed out.' }))

    const platform = within(card('Platform Management'))

    expect(platform.getByText('Unavailable')).toBeInTheDocument()
    expect(platform.queryByText('Not connected')).not.toBeInTheDocument()
    expect(platform.getByText('The platform repository timed out.')).toBeInTheDocument()
  })

  it('offers reconnect rather than connect when it is connected and broken', () => {
    panel(platformState({ value: null, error: 'The application’s key could not be read.' }))

    const platform = within(card('Platform Management'))

    expect(platform.getByRole('button', { name: 'Reconnect' })).toBeInTheDocument()
    expect(platform.queryByRole('button', { name: 'Connect' })).not.toBeInTheDocument()
    expect(platform.getByRole('button', { name: 'Disconnect' })).toBeInTheDocument()
  })

  it('says not connected when no application has been created', () => {
    panel(platformState({ value: null, unmanaged: true }), platformApplication({ application: null }))

    const platform = within(card('Platform Management'))

    expect(platform.getByText('Not connected')).toBeInTheDocument()
    // Nothing to disconnect, and nothing to reconnect to.
    expect(platform.queryByRole('button', { name: 'Disconnect' })).not.toBeInTheDocument()
  })

  it('says a deployment that manages no environment manages none', () => {
    panel(platformState({ value: null, unmanaged: true }), platformApplication({ managed: false }))

    const platform = within(card('Platform Management'))

    expect(platform.getByText('Not managed')).toBeInTheDocument()
    expect(platform.getByText(/manages no environment/i)).toBeInTheDocument()
  })

  it('offers no repository to type, only ones the installation reaches', async () => {
    // The picker asks the installation what it reaches the moment it renders.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: () =>
          Promise.resolve({
            repositories: [
              { owner: 'FieldstateNZ', name: 'saas-fabric-platform', default_branch: 'main' },
            ],
          }),
      }),
    )

    panel(
      platformState({ value: null, unmanaged: true }),
      platformApplication({ application: { ...CHOSEN, repository: null } }),
    )

    // A picker, fed from the installation. No owner field, no name field.
    expect(
      await screen.findByRole('button', { name: 'FieldstateNZ/saas-fabric-platform' }),
    ).toBeInTheDocument()
    expect(screen.queryByLabelText(/repository name/i)).not.toBeInTheDocument()
    expect(screen.queryByLabelText(/owner/i)).not.toBeInTheDocument()
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument()
  })

  it('disconnects only the integration whose button was pressed', async () => {
    const fetched = vi.fn().mockResolvedValue({ ok: true, status: 204, json: () => Promise.resolve({}) })
    vi.stubGlobal('fetch', fetched)
    // Replaced wholesale rather than spread: `location` is a class instance,
    // and only `reload` is called here.
    vi.stubGlobal('location', { reload: vi.fn() })

    panel()

    await userEvent.click(
      within(card('Platform Management')).getByRole('button', { name: 'Disconnect' }),
    )

    expect(fetched).toHaveBeenCalledTimes(1)
    expect(fetched).toHaveBeenCalledWith('/api/integrations/platform', expect.anything())
  })

  it('leaves client configuration reading as it did, whatever the platform is doing', () => {
    panel(platformState({ value: null, error: 'The platform repository timed out.' }))

    expect(within(card('Client Configuration')).getByText('Connected')).toBeInTheDocument()
  })
})
