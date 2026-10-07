/**
 * Ordering between the catalogue's reads and writes.
 *
 * Two invariants, both decided before React commits anything: only one
 * write is ever out at a time, whichever of `save` and `select` asked, and a
 * read that was already out when a write landed cannot land over it. Every
 * network call is a hand-settled promise so each test chooses the order in
 * which responses arrive; nothing here waits on a clock.
 */
import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { changeCatalogue, getCatalogue } from '../api/catalogue'
import type { CatalogueCommand, StoredCatalogue } from '../api/catalogue-types'
import type { SelectComponentVersion } from '../api/component-types'
import { ControlPlaneError } from '../api/errors'
import { useCatalogue } from './useCatalogue'

vi.mock('../api/catalogue', () => ({ getCatalogue: vi.fn(), changeCatalogue: vi.fn() }))

/** A promise the test settles by hand, after it has decided what else has happened first. */
interface Deferred<T> {
  readonly promise: Promise<T>
  readonly resolve: (value: T) => void
  readonly reject: (reason: unknown) => void
}

function deferred<T>(): Deferred<T> {
  let resolve: (value: T) => void = () => undefined
  let reject: (reason: unknown) => void = () => undefined
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

/** A fresh catalogue whose platform name says which response it was. */
function catalogueNamed(platformName: string, revision: string): StoredCatalogue {
  return {
    catalogue: {
      applications: [],
      clientFields: [],
      settings: { platformName, defaultRegion: 'New Zealand', timezone: 'Pacific/Auckland' },
      environments: [],
      activity: [],
      definitionVersion: 1,
    },
    revision,
  }
}

function settingsCommand(): CatalogueCommand {
  return {
    action: 'saveSettings',
    settings: { platformName: 'Edited', defaultRegion: 'New Zealand', timezone: 'Pacific/Auckland' },
  }
}

function selectCommand(): SelectComponentVersion {
  return {
    action: 'selectComponentVersion',
    id: 'app',
    component: 'web',
    repository: 'ghcr.io/example/web',
    version: '1.2.3',
  }
}

/** Calls `start` inside one render, so everything it kicks off sees the same committed state. */
function startTogether<T>(start: () => T): T {
  const started: { value?: T } = {}
  act(() => {
    started.value = start()
  })
  if (started.value === undefined) {
    throw new Error('startTogether: nothing was started')
  }
  return started.value
}

async function mountLoaded(initial: StoredCatalogue) {
  vi.mocked(getCatalogue).mockResolvedValueOnce(initial)
  const rendered = renderHook(() => useCatalogue())
  await waitFor(() => {
    expect(rendered.result.current.loading).toBe(false)
  })
  expect(rendered.result.current.value).toEqual(initial)
  return rendered
}

beforeEach(() => {
  vi.mocked(getCatalogue).mockReset()
  vi.mocked(changeCatalogue).mockReset()
})

describe('useCatalogue: one write at a time, whichever of save and select asked', () => {
  it('two saves in one render make one request; the second is refused without clearing saving', async () => {
    const write = deferred<StoredCatalogue>()
    vi.mocked(changeCatalogue).mockReturnValueOnce(write.promise)
    const { result } = await mountLoaded(catalogueNamed('Loaded', 'rev-1'))

    const [first, second] = startTogether(
      () => [result.current.save(settingsCommand()), result.current.save(settingsCommand())] as const,
    )

    expect(changeCatalogue).toHaveBeenCalledTimes(1)
    await expect(second).resolves.toBe(false)
    expect(result.current.saving).toBe(true)

    const saved = catalogueNamed('Saved', 'rev-2')
    await act(async () => {
      write.resolve(saved)
      await first
    })

    await expect(first).resolves.toBe(true)
    expect(result.current.value).toEqual(saved)
    expect(result.current.saving).toBe(false)
  })

  it('a select started in the same render as a save is told busy, and the save goes out alone', async () => {
    const write = deferred<StoredCatalogue>()
    vi.mocked(changeCatalogue).mockReturnValueOnce(write.promise)
    const { result } = await mountLoaded(catalogueNamed('Loaded', 'rev-1'))

    const [save, select] = startTogether(
      () => [result.current.save(settingsCommand()), result.current.select(selectCommand())] as const,
    )

    expect(changeCatalogue).toHaveBeenCalledTimes(1)
    expect(vi.mocked(changeCatalogue).mock.calls[0]?.[0]).toEqual(settingsCommand())
    await expect(select).resolves.toMatchObject({ code: 'busy' })
    expect(result.current.saving).toBe(true)

    await act(async () => {
      write.resolve(catalogueNamed('Saved', 'rev-2'))
      await save
    })
    await expect(save).resolves.toBe(true)
    expect(result.current.saving).toBe(false)
  })

  it('a save started in the same render as a select is refused, and the select goes out alone', async () => {
    const write = deferred<StoredCatalogue>()
    vi.mocked(changeCatalogue).mockReturnValueOnce(write.promise)
    const { result } = await mountLoaded(catalogueNamed('Loaded', 'rev-1'))

    const [select, save] = startTogether(
      () => [result.current.select(selectCommand()), result.current.save(settingsCommand())] as const,
    )

    expect(changeCatalogue).toHaveBeenCalledTimes(1)
    expect(vi.mocked(changeCatalogue).mock.calls[0]?.[0]).toEqual(selectCommand())
    await expect(save).resolves.toBe(false)
    expect(result.current.saving).toBe(true)

    const selected = catalogueNamed('Selected', 'rev-2')
    await act(async () => {
      write.resolve(selected)
      await select
    })
    await expect(select).resolves.toBeNull()
    expect(result.current.value).toEqual(selected)
    expect(result.current.saving).toBe(false)
  })

  it('once a write is refused by the server, the next write is allowed out', async () => {
    const first = deferred<StoredCatalogue>()
    const second = deferred<StoredCatalogue>()
    vi.mocked(changeCatalogue).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    const { result } = await mountLoaded(catalogueNamed('Loaded', 'rev-1'))

    const firstSave = startTogether(() => result.current.save(settingsCommand()))
    await act(async () => {
      first.reject(new ControlPlaneError(409, 'revision_conflict', 'stale'))
      await firstSave
    })
    await expect(firstSave).resolves.toBe(false)
    expect(result.current.conflict).toBe(true)
    expect(result.current.saving).toBe(false)

    const secondSave = startTogether(() => result.current.save(settingsCommand()))
    expect(changeCatalogue).toHaveBeenCalledTimes(2)
    expect(result.current.saving).toBe(true)

    const saved = catalogueNamed('Saved', 'rev-2')
    await act(async () => {
      second.resolve(saved)
      await secondSave
    })
    await expect(secondSave).resolves.toBe(true)
    expect(result.current.value).toEqual(saved)
    expect(result.current.conflict).toBe(false)
    expect(result.current.saveError).toBeNull()
  })
})

describe('useCatalogue: a read that was out when a write landed cannot land over it', () => {
  it('a refresh that began before the save succeeded does not replace the saved catalogue', async () => {
    const staleRead = deferred<StoredCatalogue>()
    const write = deferred<StoredCatalogue>()
    vi.mocked(changeCatalogue).mockReturnValueOnce(write.promise)
    const { result } = await mountLoaded(catalogueNamed('Loaded', 'rev-1'))

    // Queued only after the mount has consumed its own response, so this read is the refresh.
    vi.mocked(getCatalogue).mockReturnValueOnce(staleRead.promise)
    act(() => {
      result.current.refresh()
    })
    expect(getCatalogue).toHaveBeenCalledTimes(2)
    expect(result.current.loading).toBe(true)

    const saved = catalogueNamed('Saved', 'rev-2')
    const save = startTogether(() => result.current.save(settingsCommand()))
    await act(async () => {
      write.resolve(saved)
      await save
    })
    expect(result.current.value).toEqual(saved)

    await act(async () => {
      staleRead.resolve(catalogueNamed('Before the save', 'rev-1'))
      await staleRead.promise
    })
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    expect(result.current.value).toEqual(saved)
    expect(result.current.loadError).toBeNull()
  })

  it('a refresh that began before the save succeeded cannot report a load error afterwards', async () => {
    const staleRead = deferred<StoredCatalogue>()
    const write = deferred<StoredCatalogue>()
    vi.mocked(changeCatalogue).mockReturnValueOnce(write.promise)
    const { result } = await mountLoaded(catalogueNamed('Loaded', 'rev-1'))

    vi.mocked(getCatalogue).mockReturnValueOnce(staleRead.promise)
    act(() => {
      result.current.refresh()
    })
    expect(getCatalogue).toHaveBeenCalledTimes(2)
    expect(result.current.loading).toBe(true)

    const saved = catalogueNamed('Saved', 'rev-2')
    const save = startTogether(() => result.current.save(settingsCommand()))
    await act(async () => {
      write.resolve(saved)
      await save
    })

    await act(async () => {
      staleRead.reject(new Error('the network dropped the old read'))
      await staleRead.promise.catch(() => undefined)
    })
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    expect(result.current.loadError).toBeNull()
    expect(result.current.value).toEqual(saved)
  })

  it('a refresh that begins after the save succeeded still updates the catalogue', async () => {
    const write = deferred<StoredCatalogue>()
    vi.mocked(changeCatalogue).mockReturnValueOnce(write.promise)
    const { result } = await mountLoaded(catalogueNamed('Loaded', 'rev-1'))

    const saved = catalogueNamed('Saved', 'rev-2')
    const save = startTogether(() => result.current.save(settingsCommand()))
    await act(async () => {
      write.resolve(saved)
      await save
    })
    expect(result.current.value).toEqual(saved)

    const newer = catalogueNamed('Edited elsewhere', 'rev-3')
    vi.mocked(getCatalogue).mockResolvedValueOnce(newer)
    act(() => {
      result.current.refresh()
    })
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })

    expect(getCatalogue).toHaveBeenCalledTimes(2)
    expect(result.current.value).toEqual(newer)
    expect(result.current.loadError).toBeNull()
  })
})

describe('useCatalogue: a landed write is a load, and recovers from a load error', () => {
  it('a refresh that failed while the save was out stops being a load error once the save lands', async () => {
    const failedRead = deferred<StoredCatalogue>()
    const write = deferred<StoredCatalogue>()
    vi.mocked(changeCatalogue).mockReturnValueOnce(write.promise)
    const { result } = await mountLoaded(catalogueNamed('Loaded', 'rev-1'))

    vi.mocked(getCatalogue).mockReturnValueOnce(failedRead.promise)
    act(() => {
      result.current.refresh()
    })
    expect(getCatalogue).toHaveBeenCalledTimes(2)
    const save = startTogether(() => result.current.save(settingsCommand()))
    expect(result.current.loading).toBe(true)
    expect(result.current.saving).toBe(true)

    // The read fails first, while no write has landed, so its error is legitimate.
    await act(async () => {
      failedRead.reject(new Error('the network dropped the read'))
      await failedRead.promise.catch(() => undefined)
    })
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })
    expect(result.current.loadError).toBe('the network dropped the read')
    expect(result.current.saving).toBe(true)

    const saved = catalogueNamed('Saved', 'rev-2')
    await act(async () => {
      write.resolve(saved)
      await save
    })

    await expect(save).resolves.toBe(true)
    expect(result.current.saving).toBe(false)
    expect(result.current.loading).toBe(false)
    expect(result.current.value).toEqual(saved)
    expect(result.current.loadError).toBeNull()
  })

  it('a refused write leaves an existing load error alone', async () => {
    const write = deferred<StoredCatalogue>()
    vi.mocked(changeCatalogue).mockReturnValueOnce(write.promise)
    const { result } = await mountLoaded(catalogueNamed('Loaded', 'rev-1'))

    vi.mocked(getCatalogue).mockRejectedValueOnce(new Error('the network dropped the read'))
    act(() => {
      result.current.refresh()
    })
    expect(getCatalogue).toHaveBeenCalledTimes(2)
    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })
    expect(result.current.loadError).toBe('the network dropped the read')

    const save = startTogether(() => result.current.save(settingsCommand()))
    await act(async () => {
      write.reject(new ControlPlaneError(400, 'invalid', 'refused'))
      await save
    })

    await expect(save).resolves.toBe(false)
    expect(result.current.saveError).not.toBeNull()
    expect(result.current.loadError).toBe('the network dropped the read')
    expect(result.current.value).toEqual(catalogueNamed('Loaded', 'rev-1'))
  })
})
