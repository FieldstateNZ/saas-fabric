import type { ApplicationDefinition, NavigationItem } from '../api/catalogue-types'
import { Collection } from './Collection'
import { Field } from './Field'
import { Select } from './Select'

/**
 * Editing an application's navigation items — not to be confused with
 * `ApplicationEntryPointsTab`'s "Entry points", the hostname template a
 * client's callbacks are built from.
 *
 * `permission`'s hint says the application must enforce it, deliberately:
 * this console's client shell preview (see `ClientShell`) only reflects what
 * an operator has assigned, it does not check permissions, so nothing here
 * can promise that a hidden item is actually unreachable.
 */
export function NavigationEditor({
  definition,
  onChange,
}: {
  definition: ApplicationDefinition
  onChange: (items: NavigationItem[]) => void
}) {
  return (
    <Collection
      title="Navigation items"
      items={definition.navigation}
      onChange={onChange}
      label={(item) => item.label}
      create={() => ({ label: '', route: '/', feature: null, permission: '' })}
    >
      {(item, change) => (
        <>
          <Field
            label="Label"
            value={item.label}
            required
            onChange={(label) => {
              change({ ...item, label })
            }}
          />
          <Field
            label="Route"
            value={item.route}
            required
            hint="A path within the client application, such as /workspec/requirements."
            onChange={(route) => {
              change({ ...item, route })
            }}
          />
          <Select
            label="Required feature"
            value={item.feature ?? ''}
            options={[
              { value: '', label: 'Always visible' },
              ...definition.features.map((feature) => ({ value: feature.id, label: feature.name })),
            ]}
            onChange={(feature) => {
              change({ ...item, feature: feature || null })
            }}
          />
          <Field
            label="Permission"
            value={item.permission}
            hint="The application must enforce this permission."
            onChange={(permission) => {
              change({ ...item, permission })
            }}
          />
        </>
      )}
    </Collection>
  )
}
