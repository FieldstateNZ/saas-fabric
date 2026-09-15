import type { ReactNode } from 'react'

export function Brand() {
  return <span className="fabric-brand"><span className="fabric-mark" aria-hidden="true">f</span>SaaS Fabric</span>
}
export function PageHeader({ eyebrow = 'Platform', title, description, actions }: {
  eyebrow?: string; title: string; description?: string; actions?: ReactNode
}) {
  return <header className="page-header"><p className="eyebrow">{eyebrow}</p>
    <div className="page-heading"><h1>{title}</h1>{actions}</div>
    {description && <p className="page-description">{description}</p>}
  </header>
}
export function Status({ value, children }: { value: string; children?: ReactNode }) {
  return <span className={`status status--${value}`}><span aria-hidden="true">●</span>{children ?? value}</span>
}
export function EmptyState({ title, children }: { title: string; children: ReactNode }) {
  return <section className="empty-state"><span className="empty-symbol" aria-hidden="true">◇</span>
    <h2>{title}</h2><div>{children}</div></section>
}
export function Panel({ title, action, children }: { title: string; action?: ReactNode; children: ReactNode }) {
  return <section className="panel"><header className="panel-header"><h2>{title}</h2>{action}</header>{children}</section>
}
