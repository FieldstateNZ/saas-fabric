import type { ReactNode } from 'react'

/** What a {@link Collection} needs: the items themselves, and how to add, edit, and name them. */
interface CollectionProps<T> {
  readonly title: string
  readonly items: readonly T[]
  readonly onChange: (items: T[]) => void
  readonly create: () => T
  readonly label: (item: T) => string
  readonly children: (item: T, change: (item: T) => void) => ReactNode
}

/**
 * An editable list of items — components, features, plans, fields,
 * navigation entries — sharing one add/edit/remove shape so every catalogue
 * editor behaves the same way.
 *
 * Every item opens by default (`<details open>`): these lists are usually
 * short enough that collapsing them costs more clicks than it saves, and an
 * operator editing a definition is typically looking at more than one item
 * at a time.
 */
export function Collection<T>({
  title,
  items,
  onChange,
  create,
  label,
  children,
}: CollectionProps<T>) {
  return (
    <section className="collection">
      <div className="collection-heading">
        <h2>{title}</h2>
        <button
          type="button"
          onClick={() => {
            onChange([...items, create()])
          }}
        >
          + Add {title.toLowerCase().replace(/s$/, '')}
        </button>
      </div>

      {items.length === 0 && <p className="empty">No {title.toLowerCase()} yet.</p>}

      {items.map((item, index) => (
        <details className="collection-item" key={index} open>
          <summary>{label(item) || 'New item'}</summary>
          <div className="form-grid">
            {children(item, (next) => {
              onChange(items.map((existing, i) => (i === index ? next : existing)))
            })}
          </div>
          <button
            type="button"
            className="danger-link"
            onClick={() => {
              onChange(items.filter((_, i) => i !== index))
            }}
          >
            Remove {label(item) || 'item'}
          </button>
        </details>
      ))}
    </section>
  )
}
