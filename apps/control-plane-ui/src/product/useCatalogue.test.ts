import { act, renderHook, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { changeCatalogue, getCatalogue } from '../api/catalogue'
import type { StoredCatalogue } from '../api/catalogue-types'
import { ControlPlaneError } from '../api/errors'
import { useCatalogue } from './useCatalogue'

vi.mock('../api/catalogue', () => ({ getCatalogue: vi.fn(), changeCatalogue: vi.fn() }))

const stored: StoredCatalogue = {
  catalogue: {
    applications: [],
    clientFields: [],
    settings: { platformName: 'Fieldstate', defaultRegion: 'New Zealand', timezone: 'Pacific/Auckland' },
    environments: [],
    activity: [],
    definitionVersion: 1,
  },
  revision: 'rev-1',
}

describe('useCatalogue: load errors and save errors are kept apart', () => {
  it('a conflict sets saveError and conflict, and leaves loadError alone', async () => {
    vi.mocked(getCatalogue).mockResolvedValue(stored)
    vi.mocked(changeCatalogue).mockRejectedValue(
      new ControlPlaneError(409, 'revision_conflict', 'the catalogue changed since it was read'),
    )

    const { result } = renderHook(() => useCatalogue())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    await act(async () => {
      await result.current.save({ action: 'saveSettings', settings: stored.catalogue.settings })
    })

    expect(result.current.loadError).toBeNull()
    expect(result.current.saveError).toBe('the catalogue changed since it was read')
    expect(result.current.conflict).toBe(true)
  })

  it('a validation failure sets saveError but not conflict, so nothing offers to reload', async () => {
    vi.mocked(getCatalogue).mockResolvedValue(stored)
    vi.mocked(changeCatalogue).mockRejectedValue(
      new ControlPlaneError(400, 'invalid_request', 'platform name is required'),
    )

    const { result } = renderHook(() => useCatalogue())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    await act(async () => {
      await result.current.save({ action: 'saveSettings', settings: stored.catalogue.settings })
    })

    expect(result.current.saveError).toBe('platform name is required')
    expect(result.current.conflict).toBe(false)
  })

  it('refresh clears a pending save error and conflict', async () => {
    vi.mocked(getCatalogue).mockResolvedValue(stored)
    vi.mocked(changeCatalogue).mockRejectedValue(new ControlPlaneError(409, 'revision_conflict', 'stale'))

    const { result } = renderHook(() => useCatalogue())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    await act(async () => {
      await result.current.save({ action: 'saveSettings', settings: stored.catalogue.settings })
    })
    expect(result.current.conflict).toBe(true)

    await act(async () => {
      result.current.refresh()
      await waitFor(() => {
        expect(result.current.loading).toBe(false)
      })
    })

    expect(result.current.saveError).toBeNull()
    expect(result.current.conflict).toBe(false)
  })
})
