import { useEffect, useState } from 'react'

import { Field } from './Field'

/**
 * The raw text for a choice field's options, kept separate from the parsed
 * array it produces.
 *
 * The same problem `ValuesEditor` solves, for the same reason: writing the
 * parsed array straight back as `value.join(', ')` on every keystroke
 * strips a trailing comma the moment it is typed, which makes it impossible
 * to ever start a second option — the field would immediately erase the
 * separator the operator just pressed. Keeping the text local, and only
 * resyncing it when it no longer parses to `value`, lets a trailing comma
 * (and the empty option it would otherwise produce) exist while the
 * operator is mid-edit without ever being sent.
 */
export function FieldOptionsInput({
  value,
  onChange,
}: {
  value: readonly string[]
  onChange: (options: string[]) => void
}) {
  const [text, setText] = useState(() => value.join(', '))

  useEffect(() => {
    if (JSON.stringify(parseOptions(text)) !== JSON.stringify(value)) {
      setText(value.join(', '))
    }
  }, [value, text])

  return (
    <Field
      label="Options (comma separated)"
      value={text}
      onChange={(next) => {
        setText(next)
        onChange(parseOptions(next))
      }}
    />
  )
}

/** Parses the options text into a list, dropping blank entries a trailing or doubled comma would otherwise produce. */
function parseOptions(text: string): string[] {
  return text
    .split(',')
    .map((option) => option.trim())
    .filter(Boolean)
}
