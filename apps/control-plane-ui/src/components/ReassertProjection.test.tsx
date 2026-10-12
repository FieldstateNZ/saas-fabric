import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ReassertProjection } from './ReassertProjection'

describe('re-asserting an identity projection', () => {
  it('asks for confirmation before overwriting anything', () => {
    const onReassert = vi.fn(() => Promise.resolve())
    render(<ReassertProjection busy={false} onReassert={onReassert} />)

    fireEvent.click(screen.getByRole('button', { name: /re-assert configuration/i }))

    expect(onReassert).not.toHaveBeenCalled()
    expect(screen.getByText(/recorded in the audit trail/i)).toBeInTheDocument()
  })

  it('overwrites once confirmed', () => {
    const onReassert = vi.fn(() => Promise.resolve())
    render(<ReassertProjection busy={false} onReassert={onReassert} />)

    fireEvent.click(screen.getByRole('button', { name: /re-assert configuration/i }))
    fireEvent.click(screen.getByRole('button', { name: /overwrite hand edits/i }))

    expect(onReassert).toHaveBeenCalledTimes(1)
  })

  it('can be cancelled without overwriting', () => {
    const onReassert = vi.fn(() => Promise.resolve())
    render(<ReassertProjection busy={false} onReassert={onReassert} />)

    fireEvent.click(screen.getByRole('button', { name: /re-assert configuration/i }))
    fireEvent.click(screen.getByRole('button', { name: /cancel/i }))

    expect(onReassert).not.toHaveBeenCalled()
    expect(screen.getByRole('button', { name: /re-assert configuration/i })).toBeInTheDocument()
  })
})
