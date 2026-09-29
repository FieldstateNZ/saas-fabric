import type { ApplicationComponent, ComponentKind } from '../../api/component-types'
import { Select } from '../Select'

/** The kinds, in the order offered: a new component is described unless the operator chooses otherwise. */
const KINDS: readonly { readonly value: ComponentKind; readonly label: string }[] = [
  { value: 'described', label: 'described — a component artifact from a registry' },
  { value: 'container', label: 'container' },
  { value: 'helm', label: 'helm' },
  { value: 'capability', label: 'capability' },
]

/**
 * `item` as another kind: what it was typed as for the old kind does not
 * carry over, and nothing resolved does -- a described component has no
 * resolution until a version is selected for it.
 */
function asKind(item: ApplicationComponent, kind: ComponentKind): ApplicationComponent {
  const { id, name, required, policy } = item
  return { id, name, kind, reference: kind === 'capability' ? 'Identity' : '', version: '', required, policy }
}

/**
 * A component's kind: chosen for a component the saved draft does not
 * hold, and stated for one it does. The server refuses a save that changes
 * a stored component's kind, so the console does not offer one; a
 * container or chart becomes described by selecting a version for it.
 */
export function KindSelect({
  item,
  choosable,
  onChange,
}: {
  readonly item: ApplicationComponent
  readonly choosable: boolean
  readonly onChange: (item: ApplicationComponent) => void
}) {
  if (!choosable) {
    return (
      <p className="form-field">
        <span>Kind</span>
        <span>{item.kind}</span>
        <small>A saved component keeps its kind. To change it, remove it and save, then add it.</small>
      </p>
    )
  }

  return (
    <Select
      label="Kind"
      value={item.kind}
      options={KINDS}
      onChange={(value) => {
        const kind = KINDS.find((known) => known.value === value)?.value
        if (kind !== undefined) {
          onChange(asKind(item, kind))
        }
      }}
    />
  )
}
