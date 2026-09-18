import { useCallback, useEffect, useState } from 'react'

import { getPlacements, placeData } from '../api/placements'
import { ControlPlaneError, isControlPlaneError } from '../api/errors'
import type { Placements } from '../api/placement-types'
import { describe } from './useClients'

/**
 * Reading a client's data placement, and asking Fabric to place one logical
 * data source's intent (ADR 0023 part 2).
 *
 * Mirrors `useDataSources` deliberately: `unmanaged` is `platform_not_managed`
 * told apart from a load failure for the same reason it is there --
 * placements live in the same platform repository data sources do, and a
 * deployment that connects none of it cannot show either. `saveError` and
 * `conflict` are the same split `useDataSources.declare` makes between a
 * refusal the operator can read and a stale write the reload affordance
 * fixes.
 *
 * # `saveErrorLogical`
 *
 * `useDataSources` never needs this: only one form is open at a time, so its
 * `saveError` unambiguously belongs to it. The Data tab shows a `Place`
 * button on every row at once, so a `422 placement_refused` has to say which
 * row it is about. `saveErrorLogical` names the logical `place` was called
 * with when it failed -- set from that call's own argument, never read back
 * from state, so it can never end up pointing at the wrong row. `ClientDataTab`
 * reads it only when `conflict` is false: a conflict is shown once, as the
 * reload affordance, not repeated beside the row too.
 */
export interface PlacementsState {
  readonly value: Placements | null
  readonly loading: boolean
  /** Why the load failed. Never set when `unmanaged` is true. */
  readonly loadError: string | null
  readonly unmanaged: boolean
  readonly saving: boolean
  /** Why the last `place` failed. Cleared by the next `place` and by `refresh`. */
  readonly saveError: string | null
  /** Which logical `saveError` is about, when it is a refusal for one row. */
  readonly saveErrorLogical: string | null
  /** Whether `saveError` was a stale write -- the one case a reload fixes. */
  readonly conflict: boolean
  readonly refresh: () => void
  /** Places one logical data source's intent. Resolves to whether it was applied. */
  readonly place: (logical: string) => Promise<boolean>
}

export function usePlacements(clientId: string): PlacementsState {
  const [value, setValue] = useState<Placements | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [unmanaged, setUnmanaged] = useState(false)
  const [saving, setSaving] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)
  const [saveErrorLogical, setSaveErrorLogical] = useState<string | null>(null)
  const [conflict, setConflict] = useState(false)
  const [generation, setGeneration] = useState(0)

  const refresh = useCallback(() => {
    // A reload is how a caller acts on a conflict; the stale-save banner it
    // was showing no longer applies to whatever comes back.
    setSaveError(null)
    setSaveErrorLogical(null)
    setConflict(false)
    setGeneration((n) => n + 1)
  }, [])

  useEffect(() => {
    let active = true
    setLoading(true)
    setLoadError(null)
    setUnmanaged(false)

    getPlacements(clientId)
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
  }, [clientId, generation])

  const place = useCallback(
    async (logical: string) => {
      // Nothing to condition `If-Match` on yet, or a write already in
      // flight -- the same silent refusal `useDataSources.declare` makes.
      if (value === null || saving) {
        return false
      }

      setSaving(true)
      setSaveError(null)
      setSaveErrorLogical(null)
      setConflict(false)

      try {
        const next = await placeData(clientId, logical, value.revision)
        setValue(next)
        return true
      } catch (error: unknown) {
        setSaveError(describe(error))
        setSaveErrorLogical(logical)
        setConflict(isControlPlaneError(error) && error.isConflict)
        return false
      } finally {
        setSaving(false)
      }
    },
    [clientId, saving, value],
  )

  return {
    value,
    loading,
    loadError,
    unmanaged,
    saving,
    saveError,
    saveErrorLogical,
    conflict,
    refresh,
    place,
  }
}
