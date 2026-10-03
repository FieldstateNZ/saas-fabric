import type { ApplicationComponent } from '../api/component-types'
import { Check } from './Check'
import { Collection } from './Collection'
import { AuthoredComponent } from './component/AuthoredComponent'
import { DescribedComponent } from './component/DescribedComponent'
import type { ComponentSelecting } from './component/selecting'
import { isPending } from './draft-request'
import { Select } from './Select'

export type { ComponentSelecting } from './component/selecting'

/** A new component: described, unless the operator chooses another kind (ADR 0026 section 7). */
function created(): ApplicationComponent {
  return {
    id: '',
    name: '',
    kind: 'described',
    reference: '',
    version: '',
    required: false,
    policy: 'manual',
  }
}

/**
 * Editing an application's components: described components selected from
 * a registry, containers, Helm charts, and platform capabilities.
 *
 * Each kind has its own body -- `DescribedComponent` for one selected
 * through the picker, `AuthoredComponent` for the three authored as free
 * text -- and what they share is here: the update policy for anything
 * deployed, and whether every plan includes it. A described component still
 * waiting for its version has neither yet: selecting creates it with a
 * manual policy in no plan by default, and a value set before then would
 * be discarded.
 */
export function ComponentEditor({
  items,
  onChange,
  selecting,
}: {
  items: readonly ApplicationComponent[]
  onChange: (items: ApplicationComponent[]) => void
  selecting: ComponentSelecting
}) {
  return (
    <Collection<ApplicationComponent>
      title="Components"
      items={items}
      onChange={onChange}
      label={(item) => item.name || item.id}
      create={created}
    >
      {(item, change) => (
        <>
          {item.kind === 'described' ? (
            <DescribedComponent item={item} change={change} selecting={selecting} />
          ) : (
            <AuthoredComponent item={item} change={change} selecting={selecting} />
          )}
          {!isPending(item) && item.kind !== 'capability' && (
            <Select
              label="Update policy"
              value={item.policy}
              options={[
                { value: 'manual', label: 'Manual' },
                { value: 'automatic', label: 'Automatic' },
              ]}
              onChange={(policy) => {
                change({ ...item, policy: policy === 'automatic' ? 'automatic' : 'manual' })
              }}
            />
          )}
          {!isPending(item) && (
            <Check
              label="Required for all plans"
              value={item.required}
              onChange={(required) => {
                change({ ...item, required })
              }}
            />
          )}
        </>
      )}
    </Collection>
  )
}
