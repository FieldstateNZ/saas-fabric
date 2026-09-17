import type { ReactNode } from 'react'

/** What a {@link PageHeader} needs: a title always, everything else optional. */
interface PageHeaderProps {
  readonly eyebrow?: string
  readonly title: string
  readonly description?: string
  readonly actions?: ReactNode
}

/**
 * The banner almost every console page opens with.
 *
 * `eyebrow` defaults to "Platform" so a page that has not thought about it
 * still reads as part of the platform section rather than showing a blank
 * label. `actions` sits beside the title, not below it, so a page-level
 * button (new client, new application) reads as acting on this page rather
 * than as one more item in its body.
 */
export function PageHeader({ eyebrow = 'Platform', title, description, actions }: PageHeaderProps) {
  return (
    <header className="page-header">
      <p className="eyebrow">{eyebrow}</p>
      <div className="page-heading">
        <h1>{title}</h1>
        {actions}
      </div>
      {description && <p className="page-description">{description}</p>}
    </header>
  )
}
