import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { SaveNotice } from './SaveNotice'

describe('SaveNotice: reload is offered only when a caller passes it, and says what it does', () => {
  it('shows the discard-labelled reload button when a caller supplies onReload — the conflict case', () => {
    render(<SaveNotice error="the client changed since it was read" success={null} onReload={vi.fn()} />)

    expect(screen.getByText('the client changed since it was read')).toBeInTheDocument()
    expect(
      screen.getByRole('button', { name: 'Reload latest version — discards your unsaved changes' }),
    ).toBeInTheDocument()
  })

  it('offers no reload button when a caller omits onReload — a plain validation failure', () => {
    render(<SaveNotice error="platform name is required" success={null} />)

    expect(screen.getByText('platform name is required')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Reload latest version/ })).not.toBeInTheDocument()
  })
})
