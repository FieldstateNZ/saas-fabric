import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { ClientLifecycle } from '../api/lifecycle-types'
import { SAVED_INTENT } from '../api/lifecycle.fixture'
import { LifecycleStatus } from './LifecycleStatus'

const TOMBSTONE = { at: 1_700_000_000, by: 'brett@example.com', reason: 'Contract ended' }

function lifecycle(overrides: Partial<ClientLifecycle>): ClientLifecycle {
  return { ...SAVED_INTENT, unmet: [], ...overrides }
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

  it('says a tombstoned client is retained and no longer published', () => {
    render(<LifecycleStatus lifecycle={lifecycle({ state: 'tombstoned', tombstone: TOMBSTONE })} />)

    expect(screen.getByText('Tombstoned')).toBeInTheDocument()
    expect(screen.getByText(/Contract ended/)).toBeInTheDocument()
    expect(screen.getByText(/state is retained and it is no longer published/)).toBeInTheDocument()
  })

  it('says a confirmed deletion has deleted nothing', () => {
    render(
      <LifecycleStatus
        lifecycle={lifecycle({
          state: 'deletionConfirmed',
          tombstone: TOMBSTONE,
          deletionConfirmed: { at: 1_700_000_100, by: 'brett@example.com' },
        })}
      />,
    )

    expect(screen.getByText('Deletion confirmed')).toBeInTheDocument()
    expect(screen.getByText(/nothing has been deleted/)).toBeInTheDocument()
  })

  it('renders only the pill when compact', () => {
    render(<LifecycleStatus lifecycle={SAVED_INTENT} compact />)

    expect(screen.getByText('Saved, not usable yet')).toBeInTheDocument()
    expect(screen.queryByRole('list')).not.toBeInTheDocument()
  })
})
