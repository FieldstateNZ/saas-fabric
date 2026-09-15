import { act, renderHook } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { saveProduct } from '../api/catalogue'
import type { ClientProductResponse } from '../api/catalogue-types'
import { useClientFormSubmit } from './useClientFormSubmit'

vi.mock('../api/catalogue', () => ({
  createClient: vi.fn(),
  saveProduct: vi.fn(),
  getProduct: vi.fn(),
}))

const existing: ClientProductResponse = {
  client: { id: 'acme', displayName: 'Acme', hosts: [], realm: 'acme', revision: 'r1' },
  product: {
    legalName: 'Acme Ltd',
    region: 'New Zealand',
    timezone: 'Pacific/Auckland',
    definitionVersion: 1,
    configuration: {},
    applications: [],
    activity: [],
  },
  resolved: [],
  reconciliation: { status: 'applied', observedAtUnix: null, detail: null },
}

/**
 * Calling `submit` through the DOM cannot isolate the in-flight ref from
 * `fieldset disabled={busy}`, which already blocks a second *click* on a
 * disabled button before it reaches this hook at all — see `ClientForm`'s
 * own tests for that path. This calls `submit` itself, twice, with neither
 * call awaited before the second starts, which is the one thing only the
 * ref can be responsible for refusing.
 */
describe('useClientFormSubmit: a second call to submit while one is in flight is a no-op', () => {
  it('sends only one write when submit is called twice before the first resolves', async () => {
    let resolveSave: ((value: ClientProductResponse) => void) | undefined
    vi.mocked(saveProduct).mockReturnValue(
      new Promise((resolve) => {
        resolveSave = resolve
      }),
    )

    const { result } = renderHook(() =>
      useClientFormSubmit({
        id: 'acme',
        existing,
        value: {
          displayName: 'Acme',
          hosts: [],
          legalName: 'Acme Ltd',
          region: 'New Zealand',
          timezone: 'Pacific/Auckland',
          configuration: {},
          applications: [],
        },
        onSaved: vi.fn(),
        onIdTaken: vi.fn(),
      }),
    )

    await act(async () => {
      result.current.submit()
      result.current.submit()
      resolveSave?.(existing)
      await Promise.resolve()
    })

    expect(saveProduct).toHaveBeenCalledOnce()
  })
})
