import { useCallback, useEffect, useState } from 'react'

import { changeCatalogue, getCatalogue } from '../api/catalogue'
import type { CatalogueCommand, StoredCatalogue } from '../api/catalogue-types'
import { describe } from '../hooks/useClients'

/** Reading and writing the product catalogue. */
export interface CatalogueState {
  readonly value: StoredCatalogue | null
  readonly error: string | null
  readonly loading: boolean
  readonly saving: boolean
  readonly refresh: () => void
  /** Applies one command, conditioned on the last-read revision. Resolves to whether it was applied. */
  readonly save: (command: CatalogueCommand) => Promise<boolean>
}

/**
 * Loads the catalogue, and writes every command back at the revision it was
 * read at.
 *
 * `save` refuses silently — returning `false` rather than throwing — when
 * there is nothing loaded yet or a write is already in flight, so every
 * caller can `await` it without also having to guard against calling it too
 * early. A caller that wants to know *why* a save failed reads `error`
 * afterward, the same way {@link useIdentity} in `hooks/useIdentity.ts` does.
 */
export function useCatalogue(): CatalogueState {
  const [value, setValue] = useState<StoredCatalogue | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [generation, setGeneration] = useState(0)

  const refresh = useCallback(() => {
    setGeneration((n) => n + 1)
  }, [])

  useEffect(() => {
    let active = true
    setLoading(true)

    void getCatalogue()
      .then(
        (next) => {
          if (active) {
            setValue(next)
            setError(null)
          }
        },
        (error: unknown) => {
          if (active) {
            setError(describe(error))
          }
        },
      )
      .finally(() => {
        if (active) {
          setLoading(false)
        }
      })

    return () => {
      active = false
    }
  }, [generation])

  const save = async (command: CatalogueCommand) => {
    if (!value || saving) {
      return false
    }

    setSaving(true)
    setError(null)

    try {
      setValue(await changeCatalogue(command, value.revision))
      return true
    } catch (error: unknown) {
      setError(describe(error))
      return false
    } finally {
      setSaving(false)
    }
  }

  return { value, loading, error, saving, refresh, save }
}
