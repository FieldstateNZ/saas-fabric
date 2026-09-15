import type { ReactNode } from 'react'

/**
 * What a list or panel renders when it has nothing to show.
 *
 * One component so "nothing here yet" always looks the same, whether it is an
 * empty client directory, an application catalogue with no drafts, or a
 * platform that has no components to list — an operator should recognise the
 * shape rather than read every occurrence as its own small surprise.
 */
export function EmptyState({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="empty-state">
      <span className="empty-symbol" aria-hidden="true">
        ◇
      </span>
      <h2>{title}</h2>
      <div>{children}</div>
    </section>
  )
}
