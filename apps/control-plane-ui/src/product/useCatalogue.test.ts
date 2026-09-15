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
    // The message is whatever the server sends today, and is not this
    // test's business — `conflict` is derived from `isConflict`, which
    // reads the machine-readable `code`, so that is what this asserts on.
    vi.mocked(changeCatalogue).mockRejectedValue(new ControlPlaneError(409, 'revision_conflict', 'stale'))

    const { result } = renderHook(() => useCatalogue())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    await act(async () => {
      await result.current.save({ action: 'saveSettings', settings: stored.catalogue.settings })
    })

    expect(result.current.loadError).toBeNull()
    expect(result.current.saveError).not.toBeNull()
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

describe('useCatalogue: a refusal that arrives after the operator has moved on is not dropped', () => {
  it('names the page the save started on, and leaves saveError and conflict alone', async () => {
    vi.mocked(getCatalogue).mockResolvedValue(stored)
    let rejectSave: (reason: unknown) => void = () => {
      throw new Error('rejectSave called before changeCatalogue was invoked')
    }
    vi.mocked(changeCatalogue).mockReturnValue(
      new Promise((_resolve, reject) => {
        rejectSave = reject
      }),
    )

    const { result } = renderHook(() => useCatalogue())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    act(() => {
      result.current.setCurrentPage('Settings')
    })

    let savePromise: Promise<boolean> = Promise.resolve(false)
    act(() => {
      savePromise = result.current.save({ action: 'saveSettings', settings: stored.catalogue.settings })
    })

    // The operator navigates to Definition while the request above is still
    // in flight — before its response has landed.
    act(() => {
      result.current.setCurrentPage('Definition')
    })

    await act(async () => {
      rejectSave(new ControlPlaneError(409, 'revision_conflict', 'the client changed since it was read'))
      await savePromise
    })

    expect(result.current.saveError).toBeNull()
    expect(result.current.conflict).toBe(false)
    expect(result.current.navigatedAwayNotice).toBe(
      'Your change to Settings was not saved: the client changed since it was read',
    )
  })

  it('dismissNavigatedAwayNotice clears it', async () => {
    vi.mocked(getCatalogue).mockResolvedValue(stored)
    let rejectSave: (reason: unknown) => void = () => {
      throw new Error('rejectSave called before changeCatalogue was invoked')
    }
    vi.mocked(changeCatalogue).mockReturnValue(
      new Promise((_resolve, reject) => {
        rejectSave = reject
      }),
    )

    const { result } = renderHook(() => useCatalogue())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    act(() => {
      result.current.setCurrentPage('Settings')
    })
    let savePromise: Promise<boolean> = Promise.resolve(false)
    act(() => {
      savePromise = result.current.save({ action: 'saveSettings', settings: stored.catalogue.settings })
    })
    act(() => {
      result.current.setCurrentPage('Definition')
    })
    await act(async () => {
      rejectSave(new ControlPlaneError(400, 'invalid_request', 'bad input'))
      await savePromise
    })
    expect(result.current.navigatedAwayNotice).not.toBeNull()

    act(() => {
      result.current.dismissNavigatedAwayNotice()
    })

    expect(result.current.navigatedAwayNotice).toBeNull()
  })
})
