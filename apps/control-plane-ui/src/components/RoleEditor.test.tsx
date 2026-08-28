import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { RoleEditor } from './RoleEditor'

const ROLES = ['Client Realm Administrator', 'Client Realm User']

describe('the role editor', () => {
  it('offers no way to remove a role the platform requires', () => {
    // The API refuses it either way. Not offering the control is the courtesy
    // that stops an operator discovering the rule through an error.
    render(<RoleEditor roles={ROLES} disabled={false} onChange={vi.fn()} />)

    expect(screen.queryByRole('button', { name: 'Remove' })).not.toBeInTheDocument()
    expect(screen.getAllByText('required')).toHaveLength(2)
  })

  it('removes a role the platform does not require', async () => {
    const onChange = vi.fn()
    render(
      <RoleEditor roles={[...ROLES, 'Invoicing Approver']} disabled={false} onChange={onChange} />,
    )

    await userEvent.click(screen.getByRole('button', { name: 'Remove' }))

    expect(onChange).toHaveBeenCalledWith(ROLES)
  })

  it('adds a role', async () => {
    const onChange = vi.fn()
    render(<RoleEditor roles={ROLES} disabled={false} onChange={onChange} />)

    await userEvent.type(screen.getByLabelText('Add a role'), 'Invoicing Approver')
    await userEvent.click(screen.getByRole('button', { name: 'Add' }))

    expect(onChange).toHaveBeenCalledWith([...ROLES, 'Invoicing Approver'])
  })

  it('refuses to add a role that is already there', async () => {
    // A duplicate is refused by the API, and adding one here would produce a
    // list the operator could not save and could not see why.
    const onChange = vi.fn()
    render(<RoleEditor roles={ROLES} disabled={false} onChange={onChange} />)

    await userEvent.type(screen.getByLabelText('Add a role'), 'Client Realm User')
    await userEvent.click(screen.getByRole('button', { name: 'Add' }))

    expect(onChange).not.toHaveBeenCalled()
  })

  it('disables every control while a write is in flight', () => {
    render(<RoleEditor roles={[...ROLES, 'Invoicing Approver']} disabled onChange={vi.fn()} />)

    expect(screen.getByRole('button', { name: 'Remove' })).toBeDisabled()
    expect(screen.getByLabelText('Add a role')).toBeDisabled()
  })
})
