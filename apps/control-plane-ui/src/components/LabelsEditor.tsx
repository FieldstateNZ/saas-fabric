import { useEffect, useState } from 'react'

/**
 * A free-form `key=value` editor for a data source's labels.
 *
 * The same problem `ValuesEditor` (see `product/ValuesEditor.tsx`) solves,
 * for the same reason: resyncing the text only when it no longer parses to
 * `value` keeps a blank line or trailing space an operator is mid-typing
 * from being silently reformatted out from under them. Kept as its own copy
 * rather than a shared, parameterised editor -- see `FieldOptionsInput`'s
 * comment for why this codebase repeats the small parser rather than
 * generalising it across callers with different labels and different data.
 */
export function LabelsEditor({
  value,
  onChange,
}: {
  value: Readonly<Record<string, string>>
  onChange: (value: Record<string, string>) => void
}) {
  const [text, setText] = useState(() => toText(value))

  useEffect(() => {
    if (JSON.stringify(parseLabels(text)) !== JSON.stringify(value)) {
      setText(toText(value))
    }
  }, [value, text])

  return (
    <label className="form-field">
      <span>Labels (one key=value per line)</span>
      <textarea
        rows={3}
        value={text}
        onChange={(event) => {
          setText(event.target.value)
          onChange(parseLabels(event.target.value))
        }}
      />
    </label>
  )
}

function toText(value: Readonly<Record<string, string>>): string {
  return Object.entries(value)
    .map(([key, entry]) => `${key}=${entry}`)
    .join('\n')
}

function parseLabels(text: string): Record<string, string> {
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
