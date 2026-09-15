import type { ReactNode } from 'react'

/**
 * A coloured status pill.
 *
 * `value` drives the colour, via a `status--{value}` class. `console.css`
 * only defines one for each reconciliation status (`applied`, `pending`,
 * `failed`, `drifted`) and one for `connected`; anything else, including
 * `neutral` and `unknown`, falls through to the pill's plain default colour
 * rather than a class written for it. `children` lets a caller show
 * operator-facing text ("Loading", "Unavailable") while still colouring the
 * pill by the underlying value the API returned.
 */
export function Status({ value, children }: { value: string; children?: ReactNode }) {
  return (
    <span className={`status status--${value}`}>
      <span aria-hidden="true">●</span>
      {children ?? value}
    </span>
  )
}
