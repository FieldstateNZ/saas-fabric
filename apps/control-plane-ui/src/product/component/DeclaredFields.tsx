import type { ConfigurationField } from '../../api/catalogue-types'

/**
 * Configuration fields a component descriptor declares, read-only and in
 * full: every declared default is shown as it is, and a field with none
 * says so rather than showing an empty cell that could be read as "empty
 * text".
 */
export function DeclaredFields({ fields }: { readonly fields: readonly ConfigurationField[] }) {
  if (fields.length === 0) {
    return <p className="support-note">Declares no configuration fields.</p>
  }

  return (
    <div className="table-wrap">
      <table className="fabric-table">
        <thead>
          <tr>
            <th>Key</th>
            <th>Label</th>
            <th>Kind</th>
            <th>Required</th>
            <th>Default</th>
            <th>Options</th>
          </tr>
        </thead>
        <tbody>
          {fields.map((field) => (
            <tr key={field.key}>
              <td className="mono">{field.key}</td>
              <td>{field.label}</td>
              <td>{field.kind}</td>
              <td>{field.required ? 'Required' : 'Optional'}</td>
              <td className={field.default === null ? undefined : 'mono'}>
                {field.default ?? 'No default'}
              </td>
              <td>{field.options.length === 0 ? '—' : field.options.join(', ')}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
