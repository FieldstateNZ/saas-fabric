/**
 * A single labelled checkbox, used across every product editor and, via
 * {@link ChoiceSet}, for multi-select.
 *
 * `disabled` renders the checkbox inert without hiding it — `Assignments`
 * uses it for an application a client is already assigned, where removal is
 * refused until deprovisioning exists: the operator should see the
 * assignment and be told why it cannot change here, not lose the control
 * entirely.
 */
export function Check({
  label,
  value,
  onChange,
  disabled = false,
}: {
  label: string
  value: boolean
  onChange: (value: boolean) => void
  disabled?: boolean
}) {
  return (
    <label className="form-check">
      <input
        type="checkbox"
        checked={value}
        disabled={disabled}
        onChange={(event) => {
          onChange(event.target.checked)
        }}
      />
      {label}
    </label>
  )
}
