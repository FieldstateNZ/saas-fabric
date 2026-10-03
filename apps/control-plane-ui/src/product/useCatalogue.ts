import { useCallback, useEffect, useRef, useState } from 'react'

import { changeCatalogue, getCatalogue } from '../api/catalogue'
import type { CatalogueCommand, StoredCatalogue } from '../api/catalogue-types'
import type { SelectComponentVersion } from '../api/component-types'
import { ControlPlaneError, isControlPlaneError } from '../api/errors'
import { describe } from '../hooks/useClients'

/** Reading and writing the product catalogue. */
export interface CatalogueState {
  readonly value: StoredCatalogue | null
  /** Why the catalogue failed to load. Never set by a failed `save`. */
  readonly loadError: string | null
  readonly loading: boolean
  readonly saving: boolean
  /** Why the last `save` failed, while the operator is still on the page that started it. Cleared by the next `save` and by `refresh`. */
  readonly saveError: string | null
  /** Whether `saveError` was a stale write — the one case a reload actually fixes. */
  readonly conflict: boolean
  /** A refusal that arrived after the operator had already moved to a different page. Names the page it belongs to; there is nothing here to reload. */
  readonly navigatedAwayNotice: string | null
  readonly dismissNavigatedAwayNotice: () => void
  readonly refresh: () => void
  /**
   * Clears `saveError` and `conflict` without touching anything else.
   *
   * `useCatalogue` is one hook shared by every catalogue-editing page, so a
   * failure on one page is still sitting in `saveError` when an operator
   * navigates to another. `Console` calls this whenever the route changes —
   * page, client or application — so a page never opens already showing a
   * refusal that happened somewhere else.
   */
  readonly clearSaveError: () => void
  /**
   * Tells this hook which page is currently showing, by whatever label a
   * refusal for it should be reported under. `Console` keeps this current;
   * `save` reads it once, at the moment it is called, to remember where the
   * write came from — see {@link navigatedAwayNotice}.
   */
  readonly setCurrentPage: (label: string) => void
  /** Applies one command, conditioned on the last-read revision. Resolves to whether it was applied. */
  readonly save: (command: CatalogueCommand) => Promise<boolean>
  /**
   * Selects a component version (ADR 0026 section 7), conditioned on the
   * last-read revision. Resolves to `null` once the catalogue it wrote is
   * the one held -- which replaces the application's local draft -- or to
   * the refusal, which the picker that asked words for itself: its answer
   * and reason are what the operator acts on, so it is not also shown as a
   * page-level `saveError`. A refusal that lands after the operator left
   * the page becomes `navigatedAwayNotice`, as a save's does.
   */
  readonly select: (command: SelectComponentVersion) => Promise<ControlPlaneError | null>
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
 * needs `loadError` alone for its page-level banner, and every catalogue
 * editor reads `saveError` for its own `SaveNotice`.
 *
 * `saveError` and `conflict` are not scoped per editor, because this one
 * hook instance — and the one `saveError` it holds — is shared by every
 * catalogue-editing page. Left alone, a refusal on Settings would still be
 * sitting there when an operator navigated to Definition, or from one
 * application to another (`applications` is one route for every application,
 * told apart only by `applicationId`). `clearSaveError` exists for exactly
 * that: `Console` calls it whenever the page, client or application changes,
 * so an editor never opens already showing a failure from somewhere else.
 *
 * # A refusal that arrives after the operator has moved on is not dropped
 *
 * `clearSaveError` only handles a refusal that was already showing when the
 * operator navigated. It cannot help a `save` whose response has not landed
 * yet — by the time it does, the route-change effect that would have
 * cleared it has already run and moved on. `save` remembers, via
 * `setCurrentPage`, which page it was called from; if the response lands
 * after that page is no longer current, it is not a silent failure and it is
 * not shown as though it still belonged to whatever page is on screen now —
 * it becomes `navigatedAwayNotice`, naming the page it happened on, with
 * nothing to reload.
 *
 * # `conflict` is what tells a caller whether reloading helps
 *
 * Reloading fixes a stale write (`revision_conflict`) by fetching what
 * changed. It does nothing for a validation failure — the same bad request
 * would just be refused again — so only a conflict sets `conflict`, and only
 * a conflict is where a caller should offer to reload.
 *
 * `select` shares `save`'s one write path, so a selection is conditioned on
 * the same revision and lands in the same held catalogue; only who words its
 * refusal differs. That is why this file sits in the 121-150 line band: the
 * two are one write, and splitting the path from its callers would leave
 * `saving` and the navigated-away notice owned by neither.
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
  const [navigatedAwayNotice, setNavigatedAwayNotice] = useState<string | null>(null)
  const [generation, setGeneration] = useState(0)

  const currentPage = useRef('this page')
  const setCurrentPage = useCallback((label: string) => {
    currentPage.current = label
  }, [])

  const clearSaveError = useCallback(() => {
    setSaveError(null)
    setConflict(false)
  }, [])

  const refresh = useCallback(() => {
    // A reload is how a caller acts on a conflict; the stale-save banner it
    // was showing no longer applies to whatever comes back.
    clearSaveError()
    setGeneration((n) => n + 1)
  }, [clearSaveError])

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

  /** Writes `command`; `null` once applied, or the refusal and whether the page that asked is still showing. */
  const write = async (
    command: CatalogueCommand,
    revision: string | null,
  ): Promise<{ error: unknown; here: boolean } | null> => {
    const startedOn = currentPage.current
    setSaving(true)
    setSaveError(null)
    setConflict(false)
    setNavigatedAwayNotice(null)

    try {
      setValue(await changeCatalogue(command, revision))
      return null
    } catch (error: unknown) {
      const here = currentPage.current === startedOn
      if (!here) {
        setNavigatedAwayNotice(`Your change to ${startedOn} was not saved: ${describe(error)}`)
      }
      return { error, here }
    } finally {
      setSaving(false)
    }
  }

  const save = async (command: CatalogueCommand) => {
    if (!value || saving) {
      return false
    }
    const refused = await write(command, value.revision)
    if (refused?.here) {
      setSaveError(describe(refused.error))
      setConflict(isControlPlaneError(refused.error) && refused.error.isConflict)
    }
    return refused === null
  }

  const select = async (command: SelectComponentVersion) => {
    if (!value || saving) {
      return new ControlPlaneError(0, 'busy', 'Another change is still being saved.')
    }
    const refused = await write(command, value.revision)
    if (refused === null) {
      return null
    }
    return isControlPlaneError(refused.error)
      ? refused.error
      : new ControlPlaneError(0, 'unexpected', describe(refused.error))
  }

  return {
    value,
    loading,
    loadError,
    saving,
    saveError,
    conflict,
    navigatedAwayNotice,
    dismissNavigatedAwayNotice: () => {
      setNavigatedAwayNotice(null)
    },
    refresh,
    clearSaveError,
    setCurrentPage,
    save,
    select,
  }
}
