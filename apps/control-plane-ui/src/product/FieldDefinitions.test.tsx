import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { describe, expect, it } from 'vitest'

import type { ConfigurationField } from '../api/catalogue-types'
import { FieldDefinitions } from './FieldDefinitions'

function Editor() {
  const [fields, setFields] = useState<ConfigurationField[]>([
    { key: 'tier', label: 'Tier', kind: 'choice', required: false, default: null, options: ['gold'], description: '' },
  ])
  return <FieldDefinitions fields={fields} onChange={setFields} />
}

describe('FieldDefinitions: a trailing comma in options can be typed and deleted without sending an empty option', () => {
  it('keeps the raw text editable, and never commits a blank option', async () => {
    render(<Editor />)
    const options = screen.getByRole('textbox', { name: 'Options (comma separated)' })
    const user = userEvent.setup()

    await user.clear(options)
    await user.type(options, 'gold,silver,')

    // The trailing comma is still there for the operator to keep typing —
    // it was not stripped the moment it was pressed.
    expect(options).toHaveValue('gold,silver,')

    await user.type(options, '{Backspace}')
    expect(options).toHaveValue('gold,silver')
  })
})
