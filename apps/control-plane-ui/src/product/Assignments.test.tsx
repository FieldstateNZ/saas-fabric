import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { describe, expect, it } from 'vitest'

import type { AssignmentRequest, ProductApplication } from '../api/catalogue-types'
import { Assignments } from './Assignments'

function release(version: number, planIds: readonly string[], fieldKeys: readonly string[]) {
  return {
    version,
    note: `release ${String(version)}`,
    publishedAt: 1700000000 + version,
    definition: {
      name: `Portal v${String(version)}`,
      description: '',
      domain: '',
      components: [],
      features: [],
      plans: planIds.map((id) => ({ id, name: id, description: '', features: [], configuration: {} })),
      fields: fieldKeys.map((key) => ({
        key,
        label: key,
        kind: 'text' as const,
        required: false,
        default: null,
        options: [],
        description: '',
      })),
      navigation: [],
    },
  }
}

const app: ProductApplication = {
  id: 'portal',
  draft: release(3, ['starter', 'pro'], ['seats']).definition,
  releases: [release(1, ['starter', 'pro'], ['seats', 'legacy']), release(2, ['starter'], ['seats'])],
}

function Editor({ initial, locked = [] }: { initial: AssignmentRequest[]; locked?: readonly string[] }) {
  const [value, setValue] = useState(initial)
  return <Assignments apps={[app]} value={value} onChange={setValue} locked={locked} />
}

describe('Assignments: switching versions keeps what still applies', () => {
  it('keeps the plan and drops only configuration keys the new release no longer fields', async () => {
    render(
      <Editor
        initial={[{ applicationId: 'portal', version: 1, planId: 'pro', configuration: { seats: '10', legacy: 'x' } }]}
      />,
    )
    const user = userEvent.setup()

    // Release 2 keeps `pro`... wait, release 2 only defines plan `starter`.
    await user.selectOptions(screen.getByRole('combobox', { name: /version/ }), '2')

    expect(screen.getByRole('combobox', { name: /plan/ })).toHaveValue('starter')
    expect(screen.getByDisplayValue('10')).toBeInTheDocument()
    expect(screen.queryByDisplayValue('x')).not.toBeInTheDocument()
    expect(screen.getByText(/reset the plan or configuration values/)).toBeInTheDocument()
  })

  it('keeps the same plan without a note when the new release still has it', async () => {
    render(
      <Editor
        initial={[{ applicationId: 'portal', version: 1, planId: 'starter', configuration: { seats: '10' } }]}
      />,
    )
    const user = userEvent.setup()

    await user.selectOptions(screen.getByRole('combobox', { name: /version/ }), '2')

    expect(screen.getByRole('combobox', { name: /plan/ })).toHaveValue('starter')
    expect(screen.queryByText(/reset the plan or configuration values/)).not.toBeInTheDocument()
  })

  it('labels the assignment from the latest release, not the unpublished draft', () => {
    render(<Editor initial={[]} />)

    expect(screen.getByRole('checkbox', { name: 'Portal v2' })).toBeInTheDocument()
    expect(screen.queryByRole('checkbox', { name: 'Portal v3' })).not.toBeInTheDocument()
  })
})

describe('Assignments: a locked application cannot be unassigned', () => {
  it('disables the checkbox rather than silently ignoring a click', () => {
    render(
      <Editor
        initial={[{ applicationId: 'portal', version: 1, planId: 'pro', configuration: {} }]}
        locked={['portal']}
      />,
    )

    expect(screen.getByRole('checkbox', { name: 'Portal v2' })).toBeDisabled()
    expect(screen.getByText('This application is assigned. Removal requires deprovisioning.')).toBeInTheDocument()
  })
})
