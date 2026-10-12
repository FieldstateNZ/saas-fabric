import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { useState } from 'react'
import { describe, expect, it } from 'vitest'

import type { ClientProductRequest } from '../api/catalogue-types'
import { ClientDetailsStep } from './ClientDetailsStep'
import { allInternal } from './PrivateNetworkSchemeSelect'

const blank: ClientProductRequest = {
  displayName: 'Newco',
  hosts: [],
  privateNetworkScheme: null,
  legalName: 'Newco Ltd',
  region: 'NZ',
  timezone: 'Pacific/Auckland',
  configuration: {},
  applications: [],
}

/** The details step over real state, reporting every value it produces. */
function Harness({ seen }: { seen: ClientProductRequest[] }) {
  const [value, setValue] = useState(blank)
  const [hostText, setHostText] = useState('')
  return (
    <ClientDetailsStep
      id="newco"
      onIdChange={() => undefined}
      existing
      value={value}
      onValueChange={(next) => {
        seen.push(next)
        setValue(next)
      }}
      hostText={hostText}
      onHostTextChange={setHostText}
      clientFields={[]}
    />
  )
}

const schemeLabel = /Callback scheme for .internal hosts/

describe('the .internal callback scheme (D01-15)', () => {
  it('counts only hosts that are all under .internal', () => {
    expect(allInternal(['portal.lucentroot.internal', 'internal'])).toBe(true)
    expect(allInternal([])).toBe(false)
    expect(allInternal(['portal.lucentroot.internal', 'portal.example.com'])).toBe(false)
    expect(allInternal(['notinternal'])).toBe(false)
  })

  it('is asked for .internal hosts, never pre-filled, and sent as chosen', async () => {
    const seen: ClientProductRequest[] = []
    render(<Harness seen={seen} />)
    const user = userEvent.setup()

    expect(screen.queryByRole('combobox', { name: schemeLabel })).not.toBeInTheDocument()
    await user.type(screen.getByRole('textbox', { name: /Hostnames/ }), 'newco.lucentroot.internal')

    const select = screen.getByRole('combobox', { name: schemeLabel })
    expect(select).toHaveValue('')
    expect(select).toBeRequired()
    await user.selectOptions(select, 'https')

    expect(seen.at(-1)?.privateNetworkScheme).toBe('https')
  })

  it('is cleared and hidden once a host is public', async () => {
    const seen: ClientProductRequest[] = []
    render(<Harness seen={seen} />)
    const user = userEvent.setup()
    const hosts = screen.getByRole('textbox', { name: /Hostnames/ })

    await user.type(hosts, 'newco.lucentroot.internal')
    await user.selectOptions(screen.getByRole('combobox', { name: schemeLabel }), 'http')
    await user.type(hosts, ', newco.example.com')

    expect(seen.at(-1)?.privateNetworkScheme).toBeNull()
    expect(screen.queryByRole('combobox', { name: schemeLabel })).not.toBeInTheDocument()
  })
})
