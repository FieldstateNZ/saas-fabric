/** What a {@link Select} needs: a label, the current value, its options, and where changes go. */
interface SelectProps {
  readonly label: string
  readonly value: string
  readonly onChange: (value: string) => void
  readonly options: readonly { value: string; label: string }[]
  readonly required?: boolean
}

/**
 * A single labelled dropdown, used across every product editor.
 *
 * `required` matters here in a way it does not for a text input: a required
 * field whose options include a placeholder "Choose…" with an empty value
 * fails native validation while that placeholder is selected, which is
 * exactly the state it should fail in — see `ConfigurationInputs`.
 */
export function Select({ label, value, onChange, options, required = false }: SelectProps) {
  return (
    <label className="form-field">
      <span>{label}</span>
      <select
        value={value}
        required={required}
        onChange={(event) => {
          onChange(event.target.value)
        }}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  )
}
