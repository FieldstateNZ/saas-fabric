import { useCallback, useEffect, useState } from 'react'

import { changeCatalogue, getCatalogue } from '../api/catalogue'
import type { CatalogueCommand, StoredCatalogue } from '../api/catalogue-types'
import { isControlPlaneError } from '../api/errors'
import { describe } from '../hooks/useClients'

/** Reading and writing the product catalogue. */
export interface CatalogueState {
  readonly value: StoredCatalogue | null
  /** Why the catalogue failed to load. Never set by a failed `save`. */
  readonly loadError: string | null
  readonly loading: boolean
  readonly saving: boolean
  /** Why the last `save` failed. Cleared by the next `save` and by `refresh`. */
  readonly saveError: string | null
  /** Whether `saveError` was a stale write — the one case a reload actually fixes. */
  readonly conflict: boolean
  readonly refresh: () => void
  /** Applies one command, conditioned on the last-read revision. Resolves to whether it was applied. */
  readonly save: (command: CatalogueCommand) => Promise<boolean>
}

/**
 * Loads the catalogue, and writes every command back at the revision it was
 * read at.
 *
 * # Load errors and save errors are kept apart
 *
 * A failed load means the console has nothing to show; a failed save means
 * an edit was refused while the console still has a perfectly good catalogue
 * on screen. Conflating them under one `error` meant a save failure could
 * only be shown by replacing the whole page, or not shown at all — `Console`
 * needs `loadError` alone for its page-level banner, and each editor needs
 * `saveError` alone for its own.
 *
 * # `conflict` is what tells a caller whether reloading helps
 *
 * Reloading fixes a stale write (`revision_conflict`) by fetching what
 * changed. It does nothing for a validation failure — the same bad request
 * would just be refused again — so only a conflict sets `conflict`, and only
 * a conflict is where a caller should offer to reload.
 *
 * `save` refuses silently — returning `false` rather than throwing — when
 * there is nothing loaded yet or a write is already in flight, so every
 * caller can `await` it without also having to guard against calling it too
 * early.
 */
export function useCatalogue(): CatalogueState {
  const [value, setValue] = useState<StoredCatalogue | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
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

    void getCatalogue()
      .then(
        (next) => {
          if (active) {
            setValue(next)
            setLoadError(null)
          }
        },
        (error: unknown) => {
          if (active) {
            setLoadError(describe(error))
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
    setSaveError(null)
    setConflict(false)

    try {
      setValue(await changeCatalogue(command, value.revision))
      return true
    } catch (error: unknown) {
      setSaveError(describe(error))
      setConflict(isControlPlaneError(error) && error.isConflict)
      return false
    } finally {
      setSaving(false)
    }
  }

  return { value, loading, loadError, saving, saveError, conflict, refresh, save }
}
