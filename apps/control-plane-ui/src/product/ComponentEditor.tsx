import type { ApplicationComponent } from '../api/catalogue-types'
import { Check } from './Check'
import { Collection } from './Collection'
import { Field } from './Field'
import { Select } from './Select'

const kinds: ApplicationComponent['kind'][] = ['container', 'helm', 'capability']

/**
 * Platform capabilities an application can declare it needs, instead of
 * shipping its own component for it.
 */
const capabilities = [
  'Identity',
  'Database',
  'Secrets',
  'Authorization',
  'Routing',
  'Object storage',
  'Messaging',
]

/**
 * Editing an application's components: containers, Helm charts, and platform
 * capabilities.
 *
 * A capability component has no reference to build or deploy — see
 * {@link ApplicationComponent} — so switching `kind` to `capability` resets
 * `reference` to a picked-list value instead of leaving behind an image
 * reference that no longer means anything for this kind.
 */
export function ComponentEditor({
  items,
  onChange,
}: {
  items: readonly ApplicationComponent[]
  onChange: (items: ApplicationComponent[]) => void
}) {
  return (
    <Collection<ApplicationComponent>
      title="Components"
      items={items}
      onChange={onChange}
      label={(item) => item.name}
      create={() => ({
        id: '',
        name: '',
        kind: 'container',
        reference: '',
        version: '',
        required: true,
        policy: 'manual',
      })}
    >
      {(item, change) => (
        <>
          <Field
            label="Component name"
            required
            value={item.name}
            onChange={(name) => {
              change({ ...item, name })
            }}
          />
          <Field
            label="Component ID"
            required
            value={item.id}
            onChange={(id) => {
              change({ ...item, id })
            }}
          />
          <Select
            label="Kind"
            value={item.kind}
            options={kinds.map((kind) => ({ value: kind, label: kind }))}
            onChange={(value) => {
              const kind = kinds.find((kind) => kind === value)
              if (kind) {
                change({ ...item, kind, reference: kind === 'capability' ? 'Identity' : '' })
              }
            }}
          />
          {item.kind === 'capability' ? (
            <Select
              label="Capability"
              value={item.reference}
              options={capabilities.map((capability) => ({ value: capability, label: capability }))}
              onChange={(reference) => {
                change({ ...item, reference })
              }}
            />
          ) : (
            <>
              <Field
                label={item.kind === 'helm' ? 'Chart reference' : 'Image reference'}
                required
                value={item.reference}
                onChange={(reference) => {
                  change({ ...item, reference })
                }}
              />
              <Field
                label="Version or digest"
                value={item.version}
                hint="Required before publishing."
                onChange={(version) => {
                  change({ ...item, version })
                }}
              />
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
            </>
          )}
          <Check
            label="Required for all plans"
            value={item.required}
            onChange={(required) => {
              change({ ...item, required })
            }}
          />
        </>
      )}
    </Collection>
  )
}
