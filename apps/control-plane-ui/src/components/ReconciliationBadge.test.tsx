import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { Reconciliation } from '../api/types'
import { ReconciliationBadge } from './ReconciliationBadge'

/** A reconciliation state to render. */
function state(overrides: Partial<Reconciliation> = {}): Reconciliation {
  return { status: 'applied', observedAtUnix: 1_700_000_000, detail: null, ...overrides }
}

describe('the reconciliation badge', () => {
  it('says a pending change has not taken effect yet', () => {
    // The single most important sentence in the console: a written document is
    // not a converged platform service.
    render(<ReconciliationBadge reconciliation={state({ status: 'pending' })} />)

    expect(screen.getByText(/has not taken effect yet/i)).toBeInTheDocument()
  })

  it('says an applied change is in effect', () => {
    render(<ReconciliationBadge reconciliation={state()} />)

    expect(screen.getByText(/is in effect/i)).toBeInTheDocument()
  })

  it('names drift as something that happened outside SaaS Fabric', () => {
    render(<ReconciliationBadge reconciliation={state({ status: 'drifted' })} />)

    expect(screen.getByText(/outside SaaS Fabric/i)).toBeInTheDocument()
  })

  it('shows the failure detail when there is one', () => {
    render(
      <ReconciliationBadge
        reconciliation={state({ status: 'failed', detail: 'the identity provider is unavailable' })}
      />,
    )

    expect(screen.getByText(/identity provider is unavailable/i)).toBeInTheDocument()
  })

  it('says a never-checked client has not been checked rather than showing 1970', () => {
    render(<ReconciliationBadge reconciliation={state({ observedAtUnix: null })} />)

    expect(screen.getByText(/not checked yet/i)).toBeInTheDocument()
    expect(screen.queryByText(/1970/)).not.toBeInTheDocument()
  })
})
