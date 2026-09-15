import type { ReactNode } from 'react'

/**
 * A coloured status pill.
 *
 * `value` drives the colour, via a `status--{value}` class the stylesheet
 * defines for the reconciliation statuses and a handful of neutral labels —
 * it is never shown by itself unless `children` is omitted. `children` lets a
 * caller show operator-facing text ("Loading", "Unavailable") while still
 * colouring the pill by the underlying value the API returned.
 */
export function Status({ value, children }: { value: string; children?: ReactNode }) {
  return (
    <span className={`status status--${value}`}>
      <span aria-hidden="true">●</span>
      {children ?? value}
    </span>
  )
}
