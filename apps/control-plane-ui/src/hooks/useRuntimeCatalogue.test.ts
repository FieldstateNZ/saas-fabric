import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { getRuntimeCatalogue } from '../api/catalogue'
import { ControlPlaneError } from '../api/errors'
import type { RuntimeCatalogue } from '../api/runtime-catalogue-types'
import { useRuntimeCatalogue } from './useRuntimeCatalogue'

vi.mock('../api/catalogue', () => ({ getRuntimeCatalogue: vi.fn() }))
afterEach(() => {
  vi.resetAllMocks()
})

const derived: RuntimeCatalogue = {
  revision: 'rev-1',
  resources: [
    {
      name: 'customers',
      application: 'workspec',
      version: 3,
      dataSource: 'primary',
      collection: 'customers',
      keyField: 'id',
      operations: ['read', 'list', 'create'],
      queryableFields: ['id', 'name'],
    },
  ],
}

describe('useRuntimeCatalogue: loading', () => {
  it('loads what the runtime would be given right now', async () => {
    vi.mocked(getRuntimeCatalogue).mockResolvedValue(derived)

    const { result } = renderHook(() => useRuntimeCatalogue())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    expect(result.current.value).toEqual(derived)
    expect(result.current.loadError).toBeNull()
  })

  it('an empty catalogue is not a load error', async () => {
    vi.mocked(getRuntimeCatalogue).mockResolvedValue({ revision: 'rev-0', resources: [] })

    const { result } = renderHook(() => useRuntimeCatalogue())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    expect(result.current.value).toEqual({ revision: 'rev-0', resources: [] })
    expect(result.current.loadError).toBeNull()
  })

  it('a derivation conflict (500 desired_state_invalid) is a load error naming both applications', async () => {
    vi.mocked(getRuntimeCatalogue).mockRejectedValue(
      new ControlPlaneError(
        500,
        'desired_state_invalid',
        'resource "customers" is declared by both workspec and portal',
      ),
    )

    const { result } = renderHook(() => useRuntimeCatalogue())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    expect(result.current.value).toBeNull()
    expect(result.current.loadError).toBe(
      'resource "customers" is declared by both workspec and portal',
    )
  })
})

describe('useRuntimeCatalogue: refresh', () => {
  it('re-fetches, and a load error clears once the retry resolves', async () => {
    vi.mocked(getRuntimeCatalogue).mockRejectedValueOnce(
      new ControlPlaneError(500, 'desired_state_invalid', 'stale conflict'),
    )

    const { result } = renderHook(() => useRuntimeCatalogue())
    await waitFor(() => {
      expect(result.current.loadError).toBe('stale conflict')
    })

    vi.mocked(getRuntimeCatalogue).mockResolvedValue(derived)

    await act(async () => {
      result.current.refresh()
      await waitFor(() => {
        expect(result.current.loading).toBe(false)
      })
    })

    expect(result.current.value).toEqual(derived)
    expect(result.current.loadError).toBeNull()
  })
})
