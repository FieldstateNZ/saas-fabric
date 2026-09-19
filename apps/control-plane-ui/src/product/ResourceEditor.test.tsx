import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { describe, expect, it, vi } from 'vitest'

import type { ApplicationResource } from '../api/catalogue-types'
import { ResourceEditor } from './ResourceEditor'

function Editor({
  onItemsChange,
}: {
  onItemsChange?: (items: readonly ApplicationResource[]) => void
}) {
  const [items, setItems] = useState<ApplicationResource[]>([])
  return (
    <ResourceEditor
      items={items}
      onChange={(next) => {
        setItems(next)
        onItemsChange?.(next)
      }}
    />
  )
}

describe('ResourceEditor: a new resource defaults to read and list', () => {
  it('checks read and list, leaves the rest unchecked, and defaults the key field to id', async () => {
    const user = userEvent.setup()
    render(<Editor />)

    await user.click(screen.getByRole('button', { name: '+ Add resource' }))

    expect(screen.getByRole('checkbox', { name: 'read' })).toBeChecked()
    expect(screen.getByRole('checkbox', { name: 'list' })).toBeChecked()
    expect(screen.getByRole('checkbox', { name: 'create' })).not.toBeChecked()
    expect(screen.getByRole('checkbox', { name: 'update' })).not.toBeChecked()
    expect(screen.getByRole('checkbox', { name: 'delete' })).not.toBeChecked()
    expect(screen.getByRole('textbox', { name: 'Key field *' })).toHaveValue('id')
  })
})

describe('ResourceEditor: edits reach the draft', () => {
  it('carries name, logical data source and collection into onChange as they are typed', async () => {
    const onItemsChange = vi.fn()
    const user = userEvent.setup()
    render(<Editor onItemsChange={onItemsChange} />)

    await user.click(screen.getByRole('button', { name: '+ Add resource' }))
    await user.type(screen.getByRole('textbox', { name: 'Resource name *' }), 'customers')
    await user.type(screen.getByRole('textbox', { name: /^Logical data source/ }), 'primary')
    await user.type(screen.getByRole('textbox', { name: 'Collection *' }), 'customers')

    expect(onItemsChange).toHaveBeenLastCalledWith([
      expect.objectContaining({ name: 'customers', dataSource: 'primary', collection: 'customers' }),
    ])
  })

  it('toggling operations normalises the order regardless of click order', async () => {
    const onItemsChange = vi.fn()
    const user = userEvent.setup()
    render(<Editor onItemsChange={onItemsChange} />)

    await user.click(screen.getByRole('button', { name: '+ Add resource' }))
    // Clicked out of canonical order -- delete before create.
    await user.click(screen.getByRole('checkbox', { name: 'delete' }))
    await user.click(screen.getByRole('checkbox', { name: 'create' }))

    expect(onItemsChange).toHaveBeenLastCalledWith([
      expect.objectContaining({ operations: ['read', 'list', 'create', 'delete'] }),
    ])

    await user.click(screen.getByRole('checkbox', { name: 'list' }))

    expect(onItemsChange).toHaveBeenLastCalledWith([
      expect.objectContaining({ operations: ['read', 'create', 'delete'] }),
    ])
  })

  it('parses queryable fields one per line into the draft, dropping blank lines', async () => {
    const onItemsChange = vi.fn()
    const user = userEvent.setup()
    render(<Editor onItemsChange={onItemsChange} />)

    await user.click(screen.getByRole('button', { name: '+ Add resource' }))
    const fields = screen.getByRole('textbox', { name: 'Queryable fields (one per line)' })
    await user.type(fields, 'id{Enter}name{Enter}{Enter}region')

    expect(onItemsChange).toHaveBeenLastCalledWith([
      expect.objectContaining({ queryableFields: ['id', 'name', 'region'] }),
    ])
    // The blank line the operator is mid-typing is still on screen, not
    // silently collapsed out from under them.
    expect(fields).toHaveValue('id\nname\n\nregion')
  })
})

describe('ResourceEditor: Remove drops the row', () => {
  it('removes the resource from the draft', async () => {
    const user = userEvent.setup()
    render(<Editor />)

    await user.click(screen.getByRole('button', { name: '+ Add resource' }))
    await user.type(screen.getByRole('textbox', { name: 'Resource name *' }), 'customers')
    await user.click(screen.getByRole('button', { name: 'Remove customers' }))

    expect(screen.queryByRole('textbox', { name: 'Resource name *' })).not.toBeInTheDocument()
  })
})
