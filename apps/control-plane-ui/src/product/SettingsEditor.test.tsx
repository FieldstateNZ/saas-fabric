import { render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { SettingsEditor } from './SettingsEditor'
import type { CatalogueState } from './useCatalogue'

/** Only what the page reads before anything is saved. */
const STATE = {
  value: null,
  saving: false,
  saveError: null,
  conflict: false,
  refresh: () => undefined,
  save: () => Promise.resolve(false),
} as unknown as CatalogueState

/** A `fetch` answering every request with `body`, recording what was asked. */
function answering(status: number, body: unknown): ReturnType<typeof vi.fn> {
  const fetchMock = vi.fn(() =>
    Promise.resolve(
      new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } }),
    ),
  )
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

function requested(fetchMock: ReturnType<typeof vi.fn>): string[] {
  return (fetchMock.mock.calls as unknown as [string][]).map(([url]) => url)
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('SettingsEditor operator access', () => {
  it('names the signed-in operator from /api/user/current and never asks /api/operator', async () => {
    const fetchMock = answering(200, {
      subject: 'f81d4fae-7dec-11d0-a765-00a0c91e6bf6',
      userName: 'brett',
      displayName: 'Brett Example',
      email: null,
      roles: ['fabric-operator'],
    })

    render(<SettingsEditor state={STATE} />)

    expect(await screen.findByText('Brett Example')).toBeInTheDocument()
    expect(requested(fetchMock)).toEqual(['/api/user/current'])
  })

  it('shows unavailable rather than a name when the answer is not a complete user', async () => {
    answering(200, { subject: 'brett' })

    render(<SettingsEditor state={STATE} />)

    expect(await screen.findByText('Unavailable')).toBeInTheDocument()
    expect(screen.queryByText('brett')).not.toBeInTheDocument()
  })
})
