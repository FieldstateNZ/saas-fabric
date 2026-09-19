import { useEffect, useState } from 'react'

/**
 * The raw text for a resource's queryable fields, one per line.
 *
 * The same problem `ValuesEditor` and `FieldOptionsInput` solve, for the
 * same reason: reparsing the whole block on every keystroke and only
 * resyncing `text` when it no longer parses to `value` keeps a blank line an
 * operator is mid-typing from being silently reformatted out from under
 * them. Kept as its own small parser rather than a shared, parameterised
 * editor -- see `FieldOptionsInput`'s comment for why this codebase repeats
 * the pattern instead of generalising it across callers with different
 * labels and different separators.
 */
export function QueryableFieldsInput({
  value,
  onChange,
}: {
  value: readonly string[]
  onChange: (fields: string[]) => void
}) {
  const [text, setText] = useState(() => value.join('\n'))

  useEffect(() => {
    if (JSON.stringify(parseFields(text)) !== JSON.stringify(value)) {
      setText(value.join('\n'))
    }
  }, [value, text])

  return (
    <label className="form-field">
      <span>Queryable fields (one per line)</span>
      <textarea
        rows={4}
        value={text}
        onChange={(event) => {
          setText(event.target.value)
          onChange(parseFields(event.target.value))
        }}
      />
    </label>
  )
}

function parseFields(text: string): string[] {
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
}
