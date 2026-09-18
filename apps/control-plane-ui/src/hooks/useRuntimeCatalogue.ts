import { useCallback, useEffect, useState } from 'react'

import { getRuntimeCatalogue } from '../api/catalogue'
import type { RuntimeCatalogue } from '../api/runtime-catalogue-types'
import { describe } from './useClients'

/**
 * Reading the derived runtime catalogue this environment's runtime would be
 * given right now (ADR 0023 part 3).
 *
 * # Read-only, unlike `useCatalogue` and `useDataSources`
 *
 * Nothing here writes this back -- an operator changes what gets derived by
 * editing a resource on the application's own Resources tab and publishing,
 * not through this hook. So there is no `saving`, no `saveError`, and no
 * `conflict`: only `loadError`, for whatever stopped derivation or the
 * fetch itself.
 *
 * # A derivation conflict is a load error, not a write conflict
 *
 * Two different applications declaring the same resource name is refused at
 * publication, so Fabric's own writes can never produce one -- the only way
 * to reach it is a hand edit to the stored catalogue. The control plane
 * reports that the same way it reports a client document that will not
 * parse: `500 desired_state_invalid`. `refresh` re-reads exactly as it
 * would for any other load error; there is no separate conflict flag to
 * check, because nothing here was ever conditioned on a revision an
 * operator held.
 */
export interface RuntimeCatalogueState {
  readonly value: RuntimeCatalogue | null
  readonly loading: boolean
  /** Why the load failed -- including a hand-broken catalogue's `desired_state_invalid` message. */
  readonly loadError: string | null
  readonly refresh: () => void
}

export function useRuntimeCatalogue(): RuntimeCatalogueState {
  const [value, setValue] = useState<RuntimeCatalogue | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [generation, setGeneration] = useState(0)

  const refresh = useCallback(() => {
    setGeneration((n) => n + 1)
  }, [])

  useEffect(() => {
    let active = true
    setLoading(true)
    setLoadError(null)

    getRuntimeCatalogue()
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

        setValue(null)
        setLoading(false)
        setLoadError(describe(error))
      })

    return () => {
      active = false
    }
  }, [generation])

  return { value, loading, loadError, refresh }
}
