import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { ClientLifecycle, Withdrawal } from '../api/lifecycle-types'
import { SAVED_INTENT } from '../api/lifecycle.fixture'
import { LifecycleStatus } from './LifecycleStatus'

const TOMBSTONE = { at: 1_700_000_000, by: 'brett@example.com', reason: 'Contract ended' }

function lifecycle(overrides: Partial<ClientLifecycle>): ClientLifecycle {
  return { ...SAVED_INTENT, unmet: [], ...overrides }
}

function tombstoned(withdrawal: Withdrawal): ClientLifecycle {
  return lifecycle({ state: 'tombstoned', tombstone: TOMBSTONE, withdrawal })
}

/** Nothing the console shows for a retired client may claim it stopped being served. */
function expectNoServingClaim() {
  expect(screen.queryByText(/no longer published|withdrawn|stopped serving|no longer served/i)).toBeNull()
}

describe('the lifecycle status', () => {
  it('shows saved intent as not usable yet, with every unmet condition', () => {
    render(<LifecycleStatus lifecycle={SAVED_INTENT} />)

    expect(screen.getByText('Saved, not usable yet')).toBeInTheDocument()
    const unmet = screen.getByRole('list', { name: 'Not ready yet' })
    expect(unmet).toHaveTextContent('Provisioning: the client declares no data to place')
    expect(unmet).toHaveTextContent('Identity: identity has not been applied at this revision yet')
    expect(unmet).toHaveTextContent('Publication: this deployment publishes no runtime documents')
  })

  it('shows a ready client with nothing outstanding', () => {
    render(<LifecycleStatus lifecycle={lifecycle({ state: 'ready' })} />)

    expect(screen.getByText('Ready')).toBeInTheDocument()
    expect(screen.queryByRole('list', { name: 'Not ready yet' })).not.toBeInTheDocument()
  })

  it('reports a tombstone as retirement recorded, with withdrawal still pending', () => {
    render(
      <LifecycleStatus
        lifecycle={tombstoned({
          state: 'pending',
          detail:
            'the tenants document the publication target last accepted still carries this tenant, at revision 2; withdrawing a retired tenant is not built yet',
        })}
      />,
    )

    expect(screen.getByText('Retirement recorded')).toBeInTheDocument()
    expect(screen.getByText(/Contract ended\. Its state is retained\./)).toBeInTheDocument()
    expect(screen.getByText('Withdrawal pending: still published')).toBeInTheDocument()
    expect(screen.getByText(/still carries this tenant/)).toBeInTheDocument()
    expectNoServingClaim()
  })

  it('reports a refused publication pass as withdrawal not applied', () => {
    render(
      <LifecycleStatus
        lifecycle={tombstoned({
          state: 'refused',
          detail:
            'the last publication pass refused (would empty the tenants document), so the runtime keeps the documents it last accepted',
        })}
      />,
    )

    expect(screen.getByText('Withdrawal not applied: publication refused')).toBeInTheDocument()
    expect(screen.getByText(/keeps the documents it last accepted/)).toBeInTheDocument()
    expectNoServingClaim()
  })

  it('reports withdrawal as not observed when the deployment publishes nothing', () => {
    render(
      <LifecycleStatus
        lifecycle={tombstoned({
          state: 'unobserved',
          detail:
            'this deployment publishes no runtime documents, so nothing shows whether the tenant is served',
        })}
      />,
    )

    expect(screen.getByText('Withdrawal not observed')).toBeInTheDocument()
    expect(screen.getByText(/publishes no runtime documents/)).toBeInTheDocument()
    expectNoServingClaim()
  })

  it('says only what the accepted document shows when it does not carry the tenant', () => {
    render(
      <LifecycleStatus
        lifecycle={tombstoned({
          state: 'notPublished',
          detail:
            'the tenants document the publication target last accepted does not carry this tenant; no reader is observed applying it',
        })}
      />,
    )

    expect(screen.getByText('Not in the published tenants document')).toBeInTheDocument()
    expect(screen.getByText(/no reader is observed applying it/)).toBeInTheDocument()
    expectNoServingClaim()
  })

  it('says a confirmed deletion has deleted nothing', () => {
    render(
      <LifecycleStatus
        lifecycle={lifecycle({
          state: 'deletionConfirmed',
          tombstone: TOMBSTONE,
          deletionConfirmed: { at: 1_700_000_100, by: 'brett@example.com' },
          withdrawal: { state: 'pending', detail: 'still carried' },
        })}
      />,
    )

    expect(screen.getByText('Deletion confirmed (intent)')).toBeInTheDocument()
    expect(screen.getByText(/nothing has been deleted/)).toBeInTheDocument()
    expect(screen.getByText('Withdrawal pending: still published')).toBeInTheDocument()
  })

  it('shows a restored client as saved intent, keeping the tombstone it reversed', () => {
    render(
      <LifecycleStatus
        lifecycle={lifecycle({
          unmet: [{ condition: 'identity', detail: 'identity has not been applied at this revision yet' }],
          restorations: [
            { tombstone: TOMBSTONE, at: 1_700_000_200, by: 'riley@example.com', reason: 'Contract renewed' },
          ],
        })}
      />,
    )

    expect(screen.getByText('Saved, not usable yet')).toBeInTheDocument()
    expect(screen.getByRole('list', { name: 'Not ready yet' })).toHaveTextContent(
      'Identity: identity has not been applied at this revision yet',
    )
    const note = screen.getByText(/Restored .* by riley@example\.com: Contract renewed\./)
    expect(note).toHaveTextContent('(Contract ended) is kept in its history.')
    expect(screen.queryByText('Ready')).not.toBeInTheDocument()
  })

  it('keeps earlier restorations on a client retired again', () => {
    render(
      <LifecycleStatus
        lifecycle={{
          ...tombstoned({ state: 'pending', detail: 'still carried' }),
          restorations: [
            { tombstone: { ...TOMBSTONE, at: 1_600_000_000 }, at: 1_650_000_000, by: 'o', reason: 'Renewed' },
          ],
        }}
      />,
    )

    expect(screen.getByText('Retirement recorded')).toBeInTheDocument()
    expect(screen.getByText(/Contract ended\. Its state is retained\./)).toBeInTheDocument()
    expect(screen.getByText(/Restored .* by o: Renewed\./)).toBeInTheDocument()
    expectNoServingClaim()
  })

  it('says when the continuity store does not confirm the lifecycle', () => {
    render(
      <LifecycleStatus
        lifecycle={lifecycle({
          unmet: [{ condition: 'identity', detail: "the client's identity is held: no store" }],
          continuity: {
            state: 'held',
            reason: 'mismatch',
            detail: "held: the client's recorded lifecycle does not match the continuity store's",
          },
        })}
      />,
    )

    expect(screen.getByText('Lifecycle held: it does not match the confirmed history')).toBeInTheDocument()
    expect(screen.getByText(/does not match the continuity store's/)).toBeInTheDocument()
    expect(screen.queryByText('Ready')).not.toBeInTheDocument()
  })

  it('says nothing about continuity for an admitted client', () => {
    render(<LifecycleStatus lifecycle={SAVED_INTENT} />)

    expect(screen.queryByText(/Lifecycle (held|not confirmed|change)/)).not.toBeInTheDocument()
  })

  it('renders only the pill when compact', () => {
    render(<LifecycleStatus lifecycle={SAVED_INTENT} compact />)

    expect(screen.getByText('Saved, not usable yet')).toBeInTheDocument()
    expect(screen.queryByRole('list')).not.toBeInTheDocument()
  })
})
