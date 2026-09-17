import type { ConfigurationField, Values } from '../api/catalogue-types'
import { Field } from './Field'
import { Select } from './Select'

/**
 * Renders one input per configuration field, typed by the field's own
 * `kind` — a dropdown for `boolean` and `choice` alike, a plain input
 * otherwise.
 *
 * Shared by `ClientForm`, for a client's own fields, and `Assignments`, for
 * an assigned application's fields: both are editing a {@link Values} map
 * against a set of {@link ConfigurationField} definitions, and neither needs
 * to know anything about where those definitions came from.
 */
export function ConfigurationInputs({
  fields,
  values,
  onChange,
}: {
  fields: readonly ConfigurationField[]
  values: Values
  onChange: (values: Values) => void
}) {
  return (
    <div className="form-grid">
      {fields.map((field) => {
        const value = values[field.key] ?? field.default ?? ''
        const change = (value: string) => {
          onChange({ ...values, [field.key]: value })
        }

        return field.kind === 'boolean' || field.kind === 'choice' ? (
          <Select
            key={field.key}
            label={field.label + (field.required ? ' *' : '')}
            value={value}
            onChange={change}
            required={field.required}
            options={[
              { value: '', label: 'Choose…' },
              ...(field.kind === 'boolean' ? ['true', 'false'] : field.options).map((value) => ({
                value,
                label: value,
              })),
            ]}
          />
        ) : (
          <Field
            key={field.key}
            label={field.label}
            value={value}
            onChange={change}
            required={field.required}
            type={field.kind === 'number' ? 'number' : 'text'}
            hint={field.description}
          />
        )
      })}
    </div>
  )
}
