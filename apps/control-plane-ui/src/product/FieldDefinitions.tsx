import type { ConfigurationField } from '../api/catalogue-types'
import { Check } from './Check'
import { Collection } from './Collection'
import { Field } from './Field'
import { Select } from './Select'

/** Every kind of value a configuration field can hold. */
const kinds: ConfigurationField['kind'][] = [
  'text',
  'number',
  'boolean',
  'choice',
  'hostname',
  'identifier',
  'timezone',
]

/**
 * Editing a set of configuration fields — shared by `DefinitionEditor`, for
 * every client's shared fields, and `ApplicationWorkspace`'s Client
 * configuration tab, for one application's own.
 *
 * Changing `kind` clears `default`: a default that made sense for the old
 * kind (say, a number) is not guaranteed to parse under the new one, and
 * carrying it across silently would be worse than asking the operator to set
 * it again.
 */
export function FieldDefinitions({
  fields,
  onChange,
}: {
  fields: readonly ConfigurationField[]
  onChange: (fields: ConfigurationField[]) => void
}) {
  return (
    <Collection<ConfigurationField>
      title="Fields"
      items={fields}
      onChange={onChange}
      label={(field) => field.label}
      create={() => ({
        key: '',
        label: '',
        kind: 'text',
        required: false,
        default: null,
        options: [],
        description: '',
      })}
    >
      {(field, change) => (
        <>
          <Field
            label="Label"
            value={field.label}
            required
            onChange={(label) => {
              change({ ...field, label })
            }}
          />
          <Field
            label="Key"
            value={field.key}
            required
            onChange={(key) => {
              change({ ...field, key })
            }}
          />
          <Select
            label="Type"
            value={field.kind}
            options={kinds.map((kind) => ({ value: kind, label: kind }))}
            onChange={(value) => {
              const kind = kinds.find((kind) => kind === value)
              if (kind) {
                change({ ...field, kind, default: null })
              }
            }}
          />
          <Field
            label="Description"
            value={field.description}
            onChange={(description) => {
              change({ ...field, description })
            }}
          />
          <Field
            label="Default value"
            value={field.default ?? ''}
            onChange={(value) => {
              change({ ...field, default: value || null })
            }}
          />
          {field.kind === 'choice' && (
            <Field
              label="Options (comma separated)"
              value={field.options.join(', ')}
              onChange={(options) => {
                change({ ...field, options: options.split(',').map((v) => v.trim()) })
              }}
            />
          )}
          <Check
            label="Required"
            value={field.required}
            onChange={(required) => {
              change({ ...field, required })
            }}
          />
        </>
      )}
    </Collection>
  )
}
