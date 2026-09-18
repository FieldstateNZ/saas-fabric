import { useCallback, useEffect, useState } from 'react'

import { declareDataSource, getDataSources, removeDataSource } from '../api/platform'
import { ControlPlaneError, isControlPlaneError } from '../api/errors'
import type { DataSourceInput, DataSources } from '../api/data-source-types'
import { describe } from './useClients'

/**
 * Reading and declaring this environment's data sources (ADR 0023 part 1).
 *
 * # Unmanaged is not a load failure
 *
 * `GET /api/platform/data-sources` answers `platform_not_managed` on a
 * deployment that connects no platform repository at all, exactly as
 * `usePlatform` sees it. `unmanaged` carries that apart from `loadError` for
 * the same reason it does there: it is a state `DataSourcesPanel` can act on
 * (render nothing -- the page's own connect prompt already covers it), not
 * a fault to report.
 *
 * # `saveError` and `conflict` reuse `useCatalogue`'s mechanism
 *
 * A declaration the control plane refuses as invalid (422) and a stale
 * write (409) are both reported through the one `saveError` string, with
 * `conflict` true only for the second -- the same split `useCatalogue` makes
 * between a validation failure and a stale write, so `DataSourcesPanel` can
 * reuse `SaveNotice` exactly as `Environments` already does: a conflict gets
 * the reload affordance, anything else just shows the message.
 */
export interface DataSourcesState {
  readonly value: DataSources | null
  readonly loading: boolean
  /** Why the load failed. Never set when `unmanaged` is true. */
  readonly loadError: string | null
  readonly unmanaged: boolean
  readonly saving: boolean
  /** Why the last `declare` failed. Cleared by the next `declare` and by `refresh`. */
  readonly saveError: string | null
  /** Whether `saveError` was a stale write -- the one case a reload fixes. */
  readonly conflict: boolean
  readonly refresh: () => void
  /** Declares or corrects one data source. Resolves to whether it was applied. */
  readonly declare: (id: string, input: DataSourceInput) => Promise<boolean>
  /**
   * Removes a data source that no placement references. Resolves to whether
   * it was applied -- refused as `409 data_source_in_use` comes back the
   * same way a stale write does, through `saveError`.
   */
  readonly remove: (id: string) => Promise<boolean>
}

export function useDataSources(): DataSourcesState {
  const [value, setValue] = useState<DataSources | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [unmanaged, setUnmanaged] = useState(false)
  const [saving, setSaving] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)
  const [conflict, setConflict] = useState(false)
  const [generation, setGeneration] = useState(0)

  const refresh = useCallback(() => {
    // A reload is how a caller acts on a conflict; the stale-save banner it
    // was showing no longer applies to whatever comes back.
    setSaveError(null)
    setConflict(false)
    setGeneration((n) => n + 1)
  }, [])

  useEffect(() => {
    let active = true
    setLoading(true)
    setLoadError(null)
    setUnmanaged(false)

    getDataSources()
      .then((next) => {
        if (active) {
          setValue(next)
          setLoading(false)
        }
      })
      .catch((error: unknown) => {
        if (!active) {
          return
        }

        const notManaged = error instanceof ControlPlaneError && error.code === 'platform_not_managed'

        setValue(null)
        setLoading(false)
        setUnmanaged(notManaged)
        setLoadError(notManaged ? null : describe(error))
      })

    return () => {
      active = false
    }
  }, [generation])

  const declare = useCallback(
    async (id: string, input: DataSourceInput) => {
      // Nothing to condition `If-Match` on yet, or a write already in
      // flight -- the same silent refusal `useCatalogue.save` makes, so
      // every caller can `await` this without also guarding it themselves.
      if (value === null || saving) {
        return false
      }

      setSaving(true)
      setSaveError(null)
      setConflict(false)

      try {
        const next = await declareDataSource(id, input, value.revision)
        setValue(next)
        return true
      } catch (error: unknown) {
        setSaveError(describe(error))
        setConflict(isControlPlaneError(error) && error.isConflict)
        return false
      } finally {
        setSaving(false)
      }
    },
    [saving, value],
  )

  const remove = useCallback(
    async (id: string) => {
      if (value === null || saving) {
        return false
      }

      setSaving(true)
      setSaveError(null)
      setConflict(false)

      try {
        const next = await removeDataSource(id, value.revision)
        setValue(next)
        return true
      } catch (error: unknown) {
        setSaveError(describe(error))
        setConflict(isControlPlaneError(error) && error.isConflict)
        return false
      } finally {
        setSaving(false)
      }
    },
    [saving, value],
  )

  return {
    value,
    loading,
    loadError,
    unmanaged,
    saving,
    saveError,
    conflict,
    refresh,
    declare,
    remove,
  }
}
