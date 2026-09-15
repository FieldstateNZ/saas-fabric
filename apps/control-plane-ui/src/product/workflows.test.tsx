import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { useState } from 'react'
import { ValuesEditor } from './FeaturePlans'
import { ComponentEditor } from './ComponentEditor'
import { ConfigurationInputs } from './ConfigurationInputs'
import type { ApplicationComponent, Values } from '../api/catalogue-types'

describe('product editors', () => {
  it('keeps multiline limits editable while parsing values', async () => {
    const change = vi.fn()
    function Editor() { const [value, setValue] = useState<Values>({}); return <ValuesEditor value={value} onChange={(next) => { setValue(next); change(next) }} /> }
    render(<Editor />)
    await userEvent.type(screen.getByRole('textbox'), 'seats=25\nprojects=100')
    expect(screen.getByRole('textbox')).toHaveValue('seats=25\nprojects=100')
    expect(change).toHaveBeenLastCalledWith({ seats: '25', projects: '100' })
  })
  it('keeps a new component open while typing its name', async () => {
    function Editor() { const [items, setItems] = useState<ApplicationComponent[]>([]); return <ComponentEditor items={items} onChange={setItems} /> }
    render(<Editor />)
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    await userEvent.type(screen.getByRole('textbox', { name: 'Component name *' }), 'Reports')
    expect(screen.getByRole('textbox', { name: 'Component ID *' })).toBeVisible()
  })
  it('uses typed configuration controls and required defaults', async () => {
    function Editor() { const [values, setValues] = useState<Values>({}); return <ConfigurationInputs fields={[{ key: 'seats', label: 'Seats', kind: 'number', required: true, default: '10', options: [], description: '' }]} values={values} onChange={setValues} /> }
    render(<Editor />)
    const input = screen.getByRole('spinbutton')
    expect(input).toHaveValue(10)
    expect(input).toBeRequired()
    await userEvent.clear(input)
    await userEvent.type(input, '25')
    expect(input).toHaveValue(25)
  })
})
