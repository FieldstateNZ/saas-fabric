import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { Reconciliation } from '../api/types'
import { ReconciliationBadge } from './ReconciliationBadge'

/** A reconciliation state to render. */
function state(overrides: Partial<Reconciliation> = {}): Reconciliation {
  return {
    status: 'applied',
    observedAtUnix: 1_700_000_000,
    detail: null,
    handEdits: [],
    withheld: [],
    ...overrides,
  }
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

  it("lists hand edits it left in place, in an operator's words", () => {
    render(
      <ReconciliationBadge
        reconciliation={state({
          status: 'handEdited',
          handEdits: [
            { client: 'web', field: 'clientDisabled' },
            { client: 'web', field: 'declaredRedirectUriRemoved' },
          ],
        })}
      />,
    )

    expect(screen.getByText(/left in place, not overwritten/i)).toBeInTheDocument()
    expect(screen.getByText(/was switched off/i)).toBeInTheDocument()
    expect(screen.getByText(/had a declared callback removed/i)).toBeInTheDocument()
  })

  it('says a callback it cannot place was held, without calling it a hand edit', () => {
    render(
      <ReconciliationBadge
        reconciliation={state({
          status: 'handEdited',
          handEdits: [{ client: 'analytics', field: 'redirectUrisUnconfirmed' }],
        })}
      />,
    )

    expect(screen.getByText(/a change it cannot confirm it made/i)).toBeInTheDocument()
    expect(screen.getByText(/has no record of writing, so nothing was changed for it/i)).toBeInTheDocument()
  })

  it('names the projected applications it withheld', () => {
    render(<ReconciliationBadge reconciliation={state({ status: 'withheld', withheld: ['analytics'] })} />)

    expect(screen.getByText(/was not applied, and the identity provider was left as it was/i)).toBeInTheDocument()
    expect(screen.getByText('analytics')).toBeInTheDocument()
  })

  it('shows the failure detail when there is one', () => {
    render(
      <ReconciliationBadge
        reconciliation={state({
          status: 'failed',
          detail: 'the identity provider is unavailable',
        })}
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
