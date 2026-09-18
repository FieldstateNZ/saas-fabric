import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { getPlacements, placeData } from '../api/placements'
import { ControlPlaneError } from '../api/errors'
import type { PlacementEntry, Placements } from '../api/placement-types'
import { usePlacements } from './usePlacements'

vi.mock('../api/placements', () => ({ getPlacements: vi.fn(), placeData: vi.fn() }))
afterEach(() => {
  vi.resetAllMocks()
})

const placedRow: PlacementEntry = {
  logical: 'primary',
  intent: { class: 'shared', provider: 'postgres', region: 'nz' },
  placed: {
    dataSource: 'shared-postgres-nz-01',
    isolation: { kind: 'discriminator', column: 'tenant_key', value: 'acme' },
    placedAt: '2026-09-18T02:14:00Z',
  },
  refusal: null,
}

const refusedRow: PlacementEntry = {
  logical: 'audit',
  intent: { class: 'dedicated', provider: null, region: null },
  placed: null,
  refusal: 'no dedicated data source accepts a new tenant',
}

const stored: Placements = {
  clientId: 'acme',
  environment: 'lucentroot',
  revision: 'rev-1',
  placements: [placedRow, refusedRow],
}

describe('usePlacements: loading', () => {
  it("loads what this client's document asks for", async () => {
    vi.mocked(getPlacements).mockResolvedValue(stored)

    const { result } = renderHook(() => usePlacements('acme'))
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    expect(result.current.value).toEqual(stored)
    expect(result.current.unmanaged).toBe(false)
    expect(result.current.loadError).toBeNull()
    expect(getPlacements).toHaveBeenCalledWith('acme')
  })

  it('treats an unmanaged platform as a state to act on, not a load failure', async () => {
    vi.mocked(getPlacements).mockRejectedValue(
      new ControlPlaneError(404, 'platform_not_managed', 'not managed'),
    )

    const { result } = renderHook(() => usePlacements('acme'))
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    expect(result.current.unmanaged).toBe(true)
    expect(result.current.loadError).toBeNull()
    expect(result.current.value).toBeNull()
  })

  it('a hand-broken placements file (500 desired_state_invalid) is a load error, not unmanaged', async () => {
    vi.mocked(getPlacements).mockRejectedValue(
      new ControlPlaneError(500, 'desired_state_invalid', 'stored placements will not parse'),
    )

    const { result } = renderHook(() => usePlacements('acme'))
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    expect(result.current.unmanaged).toBe(false)
    expect(result.current.loadError).toBe('stored placements will not parse')
    expect(result.current.value).toBeNull()
  })
})

describe('usePlacements: place', () => {
  it('places against the document revision it read, and adopts the response', async () => {
    vi.mocked(getPlacements).mockResolvedValue(stored)
    const updated: Placements = { ...stored, revision: 'rev-2' }
    vi.mocked(placeData).mockResolvedValue(updated)

    const { result } = renderHook(() => usePlacements('acme'))
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    let applied = false
    await act(async () => {
      applied = await result.current.place('primary')
    })

    expect(applied).toBe(true)
    expect(placeData).toHaveBeenCalledWith('acme', 'primary', 'rev-1')
    expect(result.current.value).toEqual(updated)
  })

  it('a conflict sets saveError and conflict, and place resolves to false', async () => {
    vi.mocked(getPlacements).mockResolvedValue(stored)
    vi.mocked(placeData).mockRejectedValue(new ControlPlaneError(409, 'revision_conflict', 'stale'))

    const { result } = renderHook(() => usePlacements('acme'))
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    let applied = true
    await act(async () => {
      applied = await result.current.place('audit')
    })

    expect(applied).toBe(false)
    expect(result.current.saveError).toBe('stale')
    expect(result.current.conflict).toBe(true)
  })

  it('a refusal (422 placement_refused) sets saveError and names the logical, not a conflict', async () => {
    vi.mocked(getPlacements).mockResolvedValue(stored)
    vi.mocked(placeData).mockRejectedValue(
      new ControlPlaneError(422, 'placement_refused', 'no dedicated data source accepts a new tenant'),
    )

    const { result } = renderHook(() => usePlacements('acme'))
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    await act(async () => {
      await result.current.place('audit')
    })

    expect(result.current.saveError).toBe('no dedicated data source accepts a new tenant')
    expect(result.current.saveErrorLogical).toBe('audit')
    expect(result.current.conflict).toBe(false)
  })

  it('refresh clears a pending save error, its logical, and conflict', async () => {
    vi.mocked(getPlacements).mockResolvedValue(stored)
    vi.mocked(placeData).mockRejectedValue(new ControlPlaneError(409, 'revision_conflict', 'stale'))

    const { result } = renderHook(() => usePlacements('acme'))
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    await act(async () => {
      await result.current.place('primary')
    })
    expect(result.current.conflict).toBe(true)

    await act(async () => {
      result.current.refresh()
      await waitFor(() => {
        expect(result.current.loading).toBe(false)
      })
    })

    expect(result.current.saveError).toBeNull()
    expect(result.current.saveErrorLogical).toBeNull()
    expect(result.current.conflict).toBe(false)
  })

  it('refuses to place before anything has loaded', async () => {
    vi.mocked(getPlacements).mockReturnValue(new Promise(() => {}))

    const { result } = renderHook(() => usePlacements('acme'))
    expect(result.current.value).toBeNull()

    let applied = true
    await act(async () => {
      applied = await result.current.place('primary')
    })

    expect(applied).toBe(false)
    expect(placeData).not.toHaveBeenCalled()
  })
})
