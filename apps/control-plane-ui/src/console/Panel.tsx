import type { ReactNode } from 'react'

/**
 * A titled content section — the layout unit most console pages are built
 * from.
 *
 * `action`, not a second `children` slot: a panel's header can hold at most
 * one control (a status pill, a link), and giving it its own prop keeps that
 * control out of the panel body's own layout instead of requiring every
 * caller to position it by hand.
 */
export function Panel({
  title,
  action,
  children,
}: {
  title: string
  action?: ReactNode
  children: ReactNode
}) {
  return (
    <section className="panel">
      <header className="panel-header">
        <h2>{title}</h2>
        {action}
      </header>
      {children}
    </section>
  )
}
