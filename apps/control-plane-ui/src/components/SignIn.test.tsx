/**
 * The one screen an operator sees before they have signed in.
 *
 * The property under test is `canRetry`: a button that starts the console's
 * own sign-in flow must not appear when that flow is not the one that ended
 * -- a gateway session the control plane refused, where clicking it could
 * not do anything the message does not already say to do instead.
 */
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { SignIn } from './SignIn'

describe('the sign-in screen', () => {
  it('offers a way to sign in when the console can start its own flow', () => {
    render(<SignIn error={null} canRetry={true} onSignIn={vi.fn()} />)

    expect(screen.getByRole('button', { name: 'Sign in' })).toBeInTheDocument()
  })

  it('shows an error beside the button when there is one', () => {
    render(<SignIn error="Signing in failed (invalid_client)." canRetry={true} onSignIn={vi.fn()} />)

    expect(screen.getByText('Signing in failed (invalid_client).')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeInTheDocument()
  })

  it('offers no button when the flow it would start is not the one that ended', () => {
    render(
      <SignIn
        error="The gateway signed you in, but the control plane refused that sign-in. Ask an operator to check the gateway client and your operator role."
        canRetry={false}
        onSignIn={vi.fn()}
      />,
    )

    expect(screen.queryByRole('button', { name: 'Sign in' })).not.toBeInTheDocument()
    expect(screen.getByText(/the control plane refused that sign-in/)).toBeInTheDocument()
  })
})
