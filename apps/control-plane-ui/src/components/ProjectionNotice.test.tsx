import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { ProjectionNotice } from './ProjectionNotice'

describe('the projection notice', () => {
  it('shows nothing for a consistent projection', () => {
    const { container } = render(<ProjectionNotice projection={{ state: 'consistent' }} />)

    expect(container).toBeEmptyDOMElement()
  })

  it('names each diverged application and field, and nobody as the editor', () => {
    render(
      <ProjectionNotice
        projection={{
          state: 'diverged',
          divergences: [
            { client: 'analytics', field: 'redirectUris' },
            { client: 'reports', field: 'missing' },
          ],
        }}
      />,
    )

    expect(screen.getByRole('status')).toHaveTextContent(/no longer matches what this client's applications project/i)
    expect(screen.getByText(/has a different set of callbacks/i)).toBeInTheDocument()
    expect(screen.getByText(/is no longer in the stored configuration/i)).toBeInTheDocument()
    expect(screen.getByRole('status')).not.toHaveTextContent(/someone|edited by/i)
  })

  it('lists the identity clients that cannot be checked', () => {
    render(<ProjectionNotice projection={{ state: 'unverifiable', unverified: ['analytics'] }} />)

    expect(screen.getByText(/cannot be worked out/i)).toBeInTheDocument()
    expect(screen.getByText('analytics')).toBeInTheDocument()
  })
})
