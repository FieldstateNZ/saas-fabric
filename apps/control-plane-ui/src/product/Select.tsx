/** What a {@link Select} needs: a label, the current value, its options, and where changes go. */
interface SelectProps {
  readonly label: string
  readonly value: string
  readonly onChange: (value: string) => void
  readonly options: readonly { value: string; label: string }[]
}

/** A single labelled dropdown, used across every product editor. */
export function Select({ label, value, onChange, options }: SelectProps) {
  return (
    <label className="form-field">
      <span>{label}</span>
      <select
        value={value}
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
