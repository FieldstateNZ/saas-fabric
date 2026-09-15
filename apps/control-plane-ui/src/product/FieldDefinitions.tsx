import type { ConfigurationField } from '../api/catalogue-types'
import { Check, Collection, Field, Select } from './Forms'
const kinds: ConfigurationField['kind'][] = ['text', 'number', 'boolean', 'choice', 'hostname', 'identifier', 'timezone']
export function FieldDefinitions({ fields, onChange }: { fields: ConfigurationField[]; onChange: (fields: ConfigurationField[]) => void }) {
  return <Collection<ConfigurationField> title="Fields" items={fields} onChange={onChange} label={(field) => field.label} create={() => ({ key: '', label: '', kind: 'text', required: false, default: null, options: [], description: '' })}>
    {(field, change) => <><Field label="Label" value={field.label} required onChange={(label) => { change({ ...field, label }) }} />
      <Field label="Key" value={field.key} required onChange={(key) => { change({ ...field, key }) }} />
      <Select label="Type" value={field.kind} options={kinds.map((kind) => ({ value: kind, label: kind }))} onChange={(value) => { const kind = kinds.find((kind) => kind === value); if (kind) change({ ...field, kind, default: null }) }} />
      <Field label="Description" value={field.description} onChange={(description) => { change({ ...field, description }) }} />
      <Field label="Default value" value={field.default ?? ''} onChange={(value) => { change({ ...field, default: value || null }) }} />
      {field.kind === 'choice' && <Field label="Options (comma separated)" value={field.options.join(', ')} onChange={(options) => { change({ ...field, options: options.split(',').map((v) => v.trim()) }) }} />}
      <Check label="Required" value={field.required} onChange={(required) => { change({ ...field, required }) }} /></>}
  </Collection>
}
