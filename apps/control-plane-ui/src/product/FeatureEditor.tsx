import type { ApplicationDefinition, ApplicationFeature } from '../api/catalogue-types'
import { ChoiceSet } from './ChoiceSet'
import { Collection } from './Collection'
import { Field } from './Field'

/**
 * Editing an application's features: named capabilities a plan can grant.
 *
 * A feature exists independently of any plan — see `PlanEditor`, which grants
 * them — because "implemented by these components" and "granted by these
 * plans" are different questions. Folding them together would mean a
 * component change also forced a review of every plan that grants the
 * feature.
 */
export function FeatureEditor({
  definition,
  onChange,
}: {
  definition: ApplicationDefinition
  onChange: (features: ApplicationFeature[]) => void
}) {
  return (
    <Collection
      title="Features"
      items={definition.features}
      onChange={onChange}
      label={(item) => item.name}
      create={() => ({ id: '', name: '', description: '', implementedBy: [] })}
    >
      {(item, change) => (
        <>
          <Field
            label="Feature name"
            value={item.name}
            required
            onChange={(name) => {
              change({ ...item, name })
            }}
          />
          <Field
            label="Feature ID"
            value={item.id}
            required
            onChange={(id) => {
              change({ ...item, id })
            }}
          />
          <Field
            label="Description"
            value={item.description}
            onChange={(description) => {
              change({ ...item, description })
            }}
          />
          <ChoiceSet
            label="Implemented by"
            choices={definition.components}
            value={item.implementedBy}
            onChange={(implementedBy) => {
              change({ ...item, implementedBy })
            }}
          />
        </>
      )}
    </Collection>
  )
}
