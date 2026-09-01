/**
 * What an operator is told about the platform's connection.
 *
 * The distinction under test is the one the control plane went to the trouble
 * of reporting: "nobody has connected this" is not a fault, and "your access
 * was refused" is not the same as "something went wrong".
 */
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { Integration } from '../api/types'
import { IntegrationNotice } from './IntegrationNotice'

function integration(overrides: Partial<Integration>): Integration {
  return {
    status: 'connected',
    connection: null,
    last_success_at: null,
    managed: true,
    application: null,
    ...overrides,
  }
}

describe('the integration notice', () => {
  it('says nothing at all when the platform is connected', () => {
    const { container } = render(
      <IntegrationNotice integration={integration({ status: 'connected' })} />,
    )

    expect(container).toBeEmptyDOMElement()
  })

  it('offers no connection control when the deployment states its own repository', () => {
    // A deployment that names a repository has opted out. A control that would
    // overwrite that from a browser would be offering to undo it.
    render(
      <IntegrationNotice
        integration={integration({ status: 'not_configured', managed: false })}
      />,
    )

    expect(screen.queryByLabelText(/organisation/i)).not.toBeInTheDocument()
    expect(
      screen.queryByRole('button', { name: /create application/i }),
    ).not.toBeInTheDocument()
  })

  it('describes an unconnected platform without calling it an error', () => {
    render(<IntegrationNotice integration={integration({ status: 'not_configured' })} />)

    expect(screen.getByText(/No client configuration connected/i)).toBeInTheDocument()
    expect(screen.queryByText(/unreadable|refused/i)).not.toBeInTheDocument()
  })

  it('tells an operator that refused access needs attention', () => {
    render(
      <IntegrationNotice
        integration={integration({ status: 'invalid', connection: 'FieldstateNZ/saas-fabric-clients' })}
      />,
    )

    expect(screen.getByText(/needs attention/i)).toBeInTheDocument()
    expect(screen.getByText(/revoked or removed/i)).toBeInTheDocument()
    expect(screen.getByText(/FieldstateNZ\/saas-fabric-clients/)).toBeInTheDocument()
  })

  it('separates an unreadable repository from a refused one', () => {
    render(<IntegrationNotice integration={integration({ status: 'error' })} />)

    expect(screen.getByText(/unreadable/i)).toBeInTheDocument()
    expect(screen.queryByText(/needs attention/i)).not.toBeInTheDocument()
  })

  it('reports when a broken integration last worked', () => {
    render(
      <IntegrationNotice integration={integration({ status: 'invalid', last_success_at: 1_700_000_000 })} />,
    )

    expect(screen.getByText(/Last read successfully/i)).toBeInTheDocument()
  })

  it('says nothing about a last success when there has never been one', () => {
    render(<IntegrationNotice integration={integration({ status: 'error' })} />)

    expect(screen.queryByText(/Last read successfully/i)).not.toBeInTheDocument()
  })
})
