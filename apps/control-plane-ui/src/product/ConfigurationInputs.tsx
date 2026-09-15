import type { ConfigurationField, Values } from '../api/catalogue-types'
import { Field, Select } from './Forms'
export function ConfigurationInputs({ fields, values, onChange }: { fields: ConfigurationField[]; values: Values; onChange: (values: Values) => void }) {
  return <div className="form-grid">{fields.map((field) => {
    const value = values[field.key] ?? field.default ?? ''
    const change = (value: string) => { onChange({ ...values, [field.key]: value }) }
    return field.kind === 'boolean' || field.kind === 'choice'
      ? <Select key={field.key} label={field.label + (field.required ? ' *' : '')} value={value} onChange={change} options={[{ value: '', label: 'Choose…' }, ...(field.kind === 'boolean' ? ['true', 'false'] : field.options).map((value) => ({ value, label: value }))]} />
      : <Field key={field.key} label={field.label} value={value} onChange={change} required={field.required} type={field.kind === 'number' ? 'number' : 'text'} hint={field.description} />
  })}</div>
}
