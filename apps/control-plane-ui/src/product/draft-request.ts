/**
 * The local draft, and what a save sends of it (ADR 0026 section 7).
 *
 * # The local draft keeps resolutions; the request never carries one
 *
 * The console edits the definition exactly as the server stores it, so
 * `dirty` and `published` compare it with the last save and the last release
 * like with like -- a release freezes a described component's resolution,
 * and a draft stripped of it would never equal one. `toDraftRequest` strips
 * a described component's `reference`, `version` and `resolution` only when
 * it sends, because the server refuses a save that carries them rather than
 * ignoring them.
 *
 * # A component waiting for its version is not sent
 *
 * Only selecting creates a described component on the server, which refuses
 * a save naming one it holds no resolution for. Sent, a waiting component
 * would make every other edit unsaveable while it also keeps the picker
 * disabled -- each pointing the operator at the other. So a save leaves it
 * out, and the workspace keeps it locally across the save
 * (`keepingPending`), until a selection adds it or the operator removes it.
 */
import type { ApplicationDefinition } from '../api/catalogue-types'
import type {
  ApplicationComponent,
  ApplicationDraft,
  DraftComponent,
} from '../api/component-types'

/** A component as a save names it: a described one without what the server resolved. */
function toDraftComponent(component: ApplicationComponent): DraftComponent {
  const { id, name, required, policy } = component
  switch (component.kind) {
    case 'described':
      return { kind: 'described', id, name, required, policy }
    case 'container':
    case 'helm':
    case 'capability':
      return {
        kind: component.kind,
        id,
        name,
        reference: component.reference,
        version: component.version,
        required,
        policy,
      }
  }
}

/** The `saveApplication` body for `definition`: every component but those waiting for a version. */
export function toDraftRequest(definition: ApplicationDefinition): ApplicationDraft {
  return {
    ...definition,
    components: definition.components.filter((c) => !isPending(c)).map(toDraftComponent),
  }
}

/**
 * `saved`, the draft the server now holds, with the components still
 * waiting for a version in `local` appended -- those it does not hold under
 * their id. A save never sends them, and a selection that adds one answers
 * with it resolved, so neither loses what the operator is part-way through.
 */
export function keepingPending(
  saved: ApplicationDefinition,
  local: ApplicationDefinition,
): ApplicationDefinition {
  const waiting = local.components.filter(
    (c) => isPending(c) && !saved.components.some((held) => held.id === c.id),
  )
  return { ...saved, components: [...saved.components, ...waiting] }
}

/**
 * Whether `component` is described and not yet resolved: added here, and
 * waiting for a version to be selected. Only selecting creates one on the
 * server, so it is not an unsaved change a save could write.
 */
export function isPending(component: ApplicationComponent): boolean {
  return component.kind === 'described' && component.resolution === undefined
}

/**
 * Whether two definitions say the same thing. `resources` absent and
 * `resources: []` are one statement -- the server omits the key when there
 * are none -- so a resource added and removed again is not a change.
 */
export function sameDefinition(
  left: ApplicationDefinition | undefined,
  right: ApplicationDefinition | undefined,
): boolean {
  const normal = (definition: ApplicationDefinition | undefined) =>
    definition === undefined
      ? undefined
      : JSON.stringify({ ...definition, resources: definition.resources ?? [] })
  return normal(left) === normal(right)
}

/**
 * Whether `draft` differs from `saved` other than by pending components --
 * which is what keeps a version from being selected: selecting writes the
 * saved draft, and would lose every other unsaved change.
 */
export function unsavedBesidesPending(
  draft: ApplicationDefinition,
  saved: ApplicationDefinition,
): boolean {
  const settled = { ...draft, components: draft.components.filter((c) => !isPending(c)) }
  return !sameDefinition(settled, saved)
}
