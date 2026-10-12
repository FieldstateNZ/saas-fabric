import { render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { ActivityView } from '../api/catalogue-types'
import { ProductActivityFeed } from './ProductActivityFeed'

function view(omitted: number): ActivityView {
  return {
    activity: [{ at: 1_700_000_000, operator: 'brett', action: 'Client created', resource: 'acme' }],
    limit: 200,
    omitted,
  }
}

function stubActivity(body: ActivityView) {
  vi.stubGlobal(
    'fetch',
    vi.fn(() =>
      Promise.resolve(
        new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } }),
      ),
    ),
  )
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('ProductActivityFeed', () => {
  it('says it is a view, not the audit trail', async () => {
    stubActivity(view(0))

    render(<ProductActivityFeed />)

    expect(await screen.findByText('Client created')).toBeInTheDocument()
    expect(screen.getByText(/not the audit trail/)).toBeInTheDocument()
    expect(screen.queryByText(/not shown/)).not.toBeInTheDocument()
  })

  it('says how many older actions the bounded view left out', async () => {
    stubActivity(view(42))

    render(<ProductActivityFeed />)

    expect(await screen.findByText(/42 older actions are not shown/)).toBeInTheDocument()
  })
})
