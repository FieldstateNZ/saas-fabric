import { useEffect, useState } from 'react'

import type { Values } from '../api/catalogue-types'

/**
 * A free-form `key=value` editor for a plan's configuration limits.
 *
 * # Text, not a generated form
 *
 * There is no fixed list of limit keys to build fields for — a plan's limits
 * are whatever an operator decides they are — so this edits the whole map as
 * one block of text and reparses it on every keystroke, rather than offering
 * an "add limit" control one field at a time.
 *
 * # Kept in sync without fighting the operator's cursor
 *
 * The effect only rewrites `text` when it no longer parses to the `value` the
 * caller holds. An edit that still round-trips through {@link parseLimits} to
 * the same map is left alone, so a blank line or trailing space the operator
 * is mid-typing is not silently reformatted out from under them — it is only
 * `value` changing from somewhere else (switching plans, discarding a draft)
 * that resets the text.
 */
export function ValuesEditor({
  value,
  onChange,
}: {
  value: Values
  onChange: (value: Values) => void
}) {
  const [text, setText] = useState(() =>
    Object.entries(value)
      .map(([key, entry]) => `${key}=${entry}`)
      .join('\n'),
  )

  useEffect(() => {
    if (JSON.stringify(parseLimits(text)) !== JSON.stringify(value)) {
      setText(
        Object.entries(value)
          .map(([key, entry]) => `${key}=${entry}`)
          .join('\n'),
      )
    }
  }, [value, text])

  return (
    <label className="form-field">
      <span>Plan limits (one key=value per line)</span>
      <textarea
        rows={4}
        value={text}
        onChange={(event) => {
          setText(event.target.value)
          onChange(parseLimits(event.target.value))
        }}
      />
    </label>
  )
}

/**
 * Parses the editor's text into values, one `key=value` per line. A line
 * with no `=` becomes an empty value.
 */
function parseLimits(text: string): Values {
  return Object.fromEntries(
    text
      .split('\n')
      .filter((line) => line.trim())
      .map((line): [string, string] => {
        const split = line.indexOf('=')
        return split < 0 ? [line.trim(), ''] : [line.slice(0, split).trim(), line.slice(split + 1)]
      }),
  )
}
