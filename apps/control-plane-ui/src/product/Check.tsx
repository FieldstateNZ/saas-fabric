/**
 * A single labelled checkbox, used across every product editor and, via
 * {@link ChoiceSet}, for multi-select.
 */
export function Check({
  label,
  value,
  onChange,
}: {
  label: string
  value: boolean
  onChange: (value: boolean) => void
}) {
  return (
    <label className="form-check">
      <input
        type="checkbox"
        checked={value}
        onChange={(event) => {
          onChange(event.target.checked)
        }}
      />
      {label}
    </label>
  )
}
