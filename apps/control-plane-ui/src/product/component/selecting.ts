import type { ApplicationComponent } from '../../api/component-types'
import type { ControlPlaneError } from '../../api/errors'

/**
 * What the component editor needs beyond the draft: the components as last
 * saved, the local draft's, whether anything besides a pending component is
 * unsaved, and the way to select a version (ADR 0026 section 7).
 *
 * `saved` decides what a save may change: the server refuses a save that
 * gives a stored component another kind, in either direction, so a kind is
 * chosen only for a component the saved draft does not hold. `local` is
 * what a selection's id is checked against: the server selects for the
 * component its id names, so an id two local components share would reach
 * the saved one, converting or re-resolving it, instead of adding the new
 * one.
 */
export interface ComponentSelecting {
  readonly saved: readonly ApplicationComponent[]
  readonly local: readonly ApplicationComponent[]
  readonly unsaved: boolean
  readonly select: (
    component: string,
    repository: string,
    version: string,
  ) => Promise<ControlPlaneError | null>
  readonly reload: () => void
}

/** Whether the saved draft holds a component with this id. */
export function isSaved(selecting: ComponentSelecting, id: string): boolean {
  return selecting.saved.some((component) => component.id === id)
}

/** Why a version cannot be selected for component `id` now, or `null` when it can. */
export function blockedReason(selecting: ComponentSelecting, id: string): string | null {
  if (id.trim() === '') {
    return 'Give the component an ID first.'
  }
  if (selecting.local.filter((component) => component.id === id).length > 1) {
    return 'Another component already has this ID.'
  }
  if (selecting.unsaved) {
    return 'Save or discard the draft’s other changes first: selecting a version writes the saved draft, and would lose them.'
  }
  return null
}
