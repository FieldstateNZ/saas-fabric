import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { declareDataSource, getDataSources } from '../api/platform'
import { ControlPlaneError } from '../api/errors'
import type { ConnectionSelector, DataSource, DataSourceInput, DataSources } from '../api/data-source-types'
import { useDataSources } from './useDataSources'

vi.mock('../api/platform', () => ({ getDataSources: vi.fn(), declareDataSource: vi.fn() }))
afterEach(() => {
  vi.resetAllMocks()
})

const declared: DataSource = {
  id: 'shared-postgres-nz-01',
  revision: 3,
  connector: 'postgres-nz',
  connection: { kind: 'named', name: 'shared' },
  placement: 'shared',
  residency: { region: 'nz', jurisdiction: 'NZ' },
  pool: { maxConnections: 20, idleTimeoutSeconds: 300, acquireTimeoutSeconds: 5 },
  capabilities: { writable: true, acceptsNewTenants: true },
  discriminator: { column: 'tenant_key' },
  labels: { owner: 'platform' },
}

const stored: DataSources = { environment: 'lucentroot', revision: 'rev-1', dataSources: [declared] }

const input: DataSourceInput = {
  connector: declared.connector,
  // `DataSource.connection` is typed as whatever the platform reported --
  // this fixture is known to be one ADR 0023 lets an operator declare, so a
  // straight assertion (allowed in tests) names that shape rather than
  // widening `DataSourceInput.connection` to match.
  connection: declared.connection as ConnectionSelector,
  placement: declared.placement,
  residency: declared.residency,
  pool: declared.pool,
  capabilities: declared.capabilities,
  discriminator: declared.discriminator,
  labels: declared.labels,
}

describe('useDataSources: loading', () => {
  it('loads what this environment declares', async () => {
    vi.mocked(getDataSources).mockResolvedValue(stored)

    const { result } = renderHook(() => useDataSources())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    expect(result.current.value).toEqual(stored)
    expect(result.current.unmanaged).toBe(false)
    expect(result.current.loadError).toBeNull()
  })

  it('treats an unmanaged platform as a state to act on, not a load failure', async () => {
    // Mirrors `usePlatform`: `platform_not_managed` means this deployment
    // connects no platform repository at all, and `DataSourcesPanel` renders
    // nothing for it rather than reporting an error the operator cannot act on.
    vi.mocked(getDataSources).mockRejectedValue(
      new ControlPlaneError(404, 'platform_not_managed', 'not managed'),
    )

    const { result } = renderHook(() => useDataSources())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    expect(result.current.unmanaged).toBe(true)
    expect(result.current.loadError).toBeNull()
    expect(result.current.value).toBeNull()
  })

  it('a hand-broken data-sources file (500 desired_state_invalid) is a load error, not unmanaged', async () => {
    vi.mocked(getDataSources).mockRejectedValue(
      new ControlPlaneError(500, 'desired_state_invalid', 'stored data sources will not parse'),
    )

    const { result } = renderHook(() => useDataSources())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    expect(result.current.unmanaged).toBe(false)
    expect(result.current.loadError).toBe('stored data sources will not parse')
    expect(result.current.value).toBeNull()
  })
})

describe('useDataSources: declare reuses the same conflict mechanism the catalogue uses', () => {
  it('a conflict sets saveError and conflict, and declare resolves to false', async () => {
    vi.mocked(getDataSources).mockResolvedValue(stored)
    vi.mocked(declareDataSource).mockRejectedValue(
      new ControlPlaneError(409, 'revision_conflict', 'stale'),
    )

    const { result } = renderHook(() => useDataSources())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    let applied = true
    await act(async () => {
      applied = await result.current.declare(declared.id, input)
    })

    expect(applied).toBe(false)
    expect(result.current.saveError).toBe('stale')
    expect(result.current.conflict).toBe(true)
  })

  it('an invalid declaration (422) sets saveError but not conflict', async () => {
    vi.mocked(getDataSources).mockResolvedValue(stored)
    vi.mocked(declareDataSource).mockRejectedValue(
      new ControlPlaneError(422, 'invalid_data_source', 'a shared data source needs a discriminator column'),
    )

    const { result } = renderHook(() => useDataSources())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    await act(async () => {
      await result.current.declare(declared.id, input)
    })

    expect(result.current.saveError).toBe('a shared data source needs a discriminator column')
    expect(result.current.conflict).toBe(false)
  })

  it('a hand-broken data-sources file (500 desired_state_invalid) sets saveError with no reload affordance', async () => {
    // The same answer a client document that will not parse already gets:
    // not a stale write, so `conflict` stays false and there is nothing a
    // reload would fix.
    vi.mocked(getDataSources).mockResolvedValue(stored)
    vi.mocked(declareDataSource).mockRejectedValue(
      new ControlPlaneError(500, 'desired_state_invalid', 'stored data sources will not parse'),
    )

    const { result } = renderHook(() => useDataSources())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    await act(async () => {
      await result.current.declare(declared.id, input)
    })

    expect(result.current.saveError).toBe('stored data sources will not parse')
    expect(result.current.conflict).toBe(false)
  })
})

describe('useDataSources: refresh and the write precondition', () => {
  it('refresh clears a pending save error and conflict', async () => {
    vi.mocked(getDataSources).mockResolvedValue(stored)
    vi.mocked(declareDataSource).mockRejectedValue(
      new ControlPlaneError(409, 'revision_conflict', 'stale'),
    )

    const { result } = renderHook(() => useDataSources())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    await act(async () => {
      await result.current.declare(declared.id, input)
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

  it('declares against the document revision it read, and adopts the response', async () => {
    vi.mocked(getDataSources).mockResolvedValue(stored)
    const updated: DataSources = { ...stored, revision: 'rev-2' }
    vi.mocked(declareDataSource).mockResolvedValue(updated)

    const { result } = renderHook(() => useDataSources())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    let applied = false
    await act(async () => {
      applied = await result.current.declare(declared.id, input)
    })

    expect(applied).toBe(true)
    expect(declareDataSource).toHaveBeenCalledWith(declared.id, input, 'rev-1')
    expect(result.current.value).toEqual(updated)
  })

  it('sends If-Match with the tag GET returned, for the very first declaration in an empty environment', async () => {
    // The late-bound binding always has a generation tag to compare-and-swap
    // on, even before any file exists -- so an empty environment still reads
    // a non-null revision, and the first declaration conditions on exactly
    // that tag rather than skipping the precondition.
    const empty: DataSources = { environment: 'lucentroot', revision: 'rev-0', dataSources: [] }
    vi.mocked(getDataSources).mockResolvedValue(empty)
    vi.mocked(declareDataSource).mockResolvedValue(stored)

    const { result } = renderHook(() => useDataSources())
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    await act(async () => {
      await result.current.declare(declared.id, input)
    })

    expect(declareDataSource).toHaveBeenCalledWith(declared.id, input, 'rev-0')
  })

  it('refuses to declare before anything has loaded', async () => {
    vi.mocked(getDataSources).mockReturnValue(new Promise(() => {}))

    const { result } = renderHook(() => useDataSources())
    expect(result.current.value).toBeNull()

    let applied = true
    await act(async () => {
      applied = await result.current.declare(declared.id, input)
    })

    expect(applied).toBe(false)
    expect(declareDataSource).not.toHaveBeenCalled()
  })
})
