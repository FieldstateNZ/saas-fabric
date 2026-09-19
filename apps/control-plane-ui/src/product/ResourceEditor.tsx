import type { ApplicationResource, OperationKind } from '../api/catalogue-types'
import { ChoiceSet } from './ChoiceSet'
import { Collection } from './Collection'
import { Field } from './Field'
import { QueryableFieldsInput } from './QueryableFieldsInput'

/** Every operation a resource may permit, offered in this fixed order. */
const operationChoices: readonly { id: OperationKind; name: string }[] = [
  { id: 'read', name: 'read' },
  { id: 'list', name: 'list' },
  { id: 'create', name: 'create' },
  { id: 'update', name: 'update' },
  { id: 'delete', name: 'delete' },
]

/**
 * Editing an application's resources: the logical shapes it exposes through
 * the Data API (ADR 0023 part 3), and where each one's rows actually live.
 *
 * `dataSource` is a free-text field, not a picker over `Environments`' own
 * declared data sources -- it names the *logical* source a client's own
 * `spec.data` uses, which this console has no reason to resolve here. A new
 * resource defaults to `read` and `list`, the two operations that need
 * nothing beyond a key field to serve safely; `create`, `update` and
 * `delete` are the operator's explicit choice to add.
 *
 * # `ChoiceSet`'s selection is normalised back to `OperationKind`
 *
 * `ChoiceSet` speaks in plain `string[]`, so its `onChange` cannot hand back
 * something already known to be `OperationKind[]`. Filtering the fixed,
 * already-typed `operationChoices` list by membership in what came back
 * narrows it honestly -- without an `as` assertion -- and normalises the
 * result to `operationChoices`' own order regardless of the order the
 * operator clicked in.
 */
export function ResourceEditor({
  items,
  onChange,
}: {
  items: readonly ApplicationResource[]
  onChange: (items: ApplicationResource[]) => void
}) {
  return (
    <Collection<ApplicationResource>
      title="Resources"
      items={items}
      onChange={onChange}
      label={(item) => item.name}
      create={() => ({
        name: '',
        dataSource: '',
        collection: '',
        keyField: 'id',
        operations: ['read', 'list'],
        queryableFields: [],
      })}
    >
      {(item, change) => (
        <>
          <Field
            label="Resource name"
            required
            value={item.name}
            onChange={(name) => {
              change({ ...item, name })
            }}
          />
          <Field
            label="Logical data source"
            required
            value={item.dataSource}
            hint="The name this application's clients declare in spec.data."
            onChange={(dataSource) => {
              change({ ...item, dataSource })
            }}
          />
          <Field
            label="Collection"
            required
            value={item.collection}
            onChange={(collection) => {
              change({ ...item, collection })
            }}
          />
          <Field
            label="Key field"
            required
            value={item.keyField}
            onChange={(keyField) => {
              change({ ...item, keyField })
            }}
          />
          <ChoiceSet
            label="Operations"
            choices={operationChoices}
            value={item.operations}
            onChange={(selected) => {
              change({
                ...item,
                operations: operationChoices
                  .map((operation) => operation.id)
                  .filter((id) => selected.includes(id)),
              })
            }}
          />
          <QueryableFieldsInput
            value={item.queryableFields}
            onChange={(queryableFields) => {
              change({ ...item, queryableFields })
            }}
          />
        </>
      )}
    </Collection>
  )
}
