import type { ApplicationDefinition, ApplicationPlan } from '../api/catalogue-types'
import { ChoiceSet } from './ChoiceSet'
import { Collection } from './Collection'
import { Field } from './Field'
import { ValuesEditor } from './ValuesEditor'

/**
 * Editing an application's plans: which features each plan grants, and the
 * configuration limits that come with it.
 *
 * A plan's `configuration` is edited through {@link ValuesEditor} rather than
 * one {@link Field} per key, because a plan's limits are open-ended — an
 * operator can introduce a new key this console has never seen without this
 * editor needing to know its name in advance.
 */
export function PlanEditor({
  definition,
  onChange,
}: {
  definition: ApplicationDefinition
  onChange: (plans: ApplicationPlan[]) => void
}) {
  return (
    <Collection
      title="Plans"
      items={definition.plans}
      onChange={onChange}
      label={(item) => item.name}
      create={() => ({ id: '', name: '', description: '', features: [], configuration: {} })}
    >
      {(item, change) => (
        <>
          <Field
            label="Plan name"
            value={item.name}
            required
            onChange={(name) => {
              change({ ...item, name })
            }}
          />
          <Field
            label="Plan ID"
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
            label="Features granted"
            choices={definition.features}
            value={item.features}
            onChange={(features) => {
              change({ ...item, features })
            }}
          />
          <ValuesEditor
            value={item.configuration}
            onChange={(configuration) => {
              change({ ...item, configuration })
            }}
          />
        </>
      )}
    </Collection>
  )
}
