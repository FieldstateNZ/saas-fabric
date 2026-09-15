import type { ApplicationComponent } from '../api/catalogue-types'
import { Check, Collection, Field, Select } from './Forms'
const kinds: ApplicationComponent['kind'][] = ['container', 'helm', 'capability']
const capabilities = ['Identity', 'Database', 'Secrets', 'Authorization', 'Routing', 'Object storage', 'Messaging']
export function ComponentEditor({ items, onChange }: { items: ApplicationComponent[]; onChange: (items: ApplicationComponent[]) => void }) {
  return <Collection<ApplicationComponent> title="Components" items={items} onChange={onChange} label={(item) => item.name} create={() => ({ id: '', name: '', kind: 'container', reference: '', version: '', required: true, policy: 'manual' })}>
    {(item, change) => <><Field label="Component name" required value={item.name} onChange={(name) => { change({ ...item, name }) }} />
      <Field label="Component ID" required value={item.id} onChange={(id) => { change({ ...item, id }) }} />
      <Select label="Kind" value={item.kind} options={kinds.map((kind) => ({ value: kind, label: kind }))} onChange={(value) => { const kind = kinds.find((kind) => kind === value); if (kind) change({ ...item, kind, reference: kind === 'capability' ? 'Identity' : '' }) }} />
      {item.kind === 'capability' ? <Select label="Capability" value={item.reference} options={capabilities.map((capability) => ({ value: capability, label: capability }))} onChange={(reference) => { change({ ...item, reference }) }} />
        : <><Field label={item.kind === 'helm' ? 'Chart reference' : 'Image reference'} required value={item.reference} onChange={(reference) => { change({ ...item, reference }) }} />
          <Field label="Version or digest" value={item.version} hint="Required before publishing." onChange={(version) => { change({ ...item, version }) }} />
          <Select label="Update policy" value={item.policy} options={[{ value: 'manual', label: 'Manual' }, { value: 'automatic', label: 'Automatic' }]} onChange={(policy) => { change({ ...item, policy: policy === 'automatic' ? 'automatic' : 'manual' }) }} /></>}
      <Check label="Required for all plans" value={item.required} onChange={(required) => { change({ ...item, required }) }} /></>}
  </Collection>
}
