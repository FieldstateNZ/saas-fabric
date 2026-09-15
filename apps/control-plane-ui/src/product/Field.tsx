/** What a {@link Field} needs: a label and a value always, the rest optional. */
interface FieldProps {
  readonly label: string
  readonly value: string
  readonly onChange: (value: string) => void
  readonly required?: boolean
  readonly type?: string
  readonly hint?: string
}

/**
 * A single labelled text input, used across every product editor.
 *
 * `maxLength={4096}` is a client-side courtesy, not the validation: the
 * control plane enforces its own limits on every field, and this exists only
 * so an operator sees the cap while typing rather than after a refusal.
 */
export function Field({
  label,
  value,
  onChange,
  required = false,
  type = 'text',
  hint,
}: FieldProps) {
  return (
    <label className="form-field">
      <span>
        {label}
        {required && ' *'}
      </span>
      <input
        type={type}
        value={value}
        required={required}
        maxLength={4096}
        onChange={(event) => {
          onChange(event.target.value)
        }}
      />
      {hint && <small>{hint}</small>}
    </label>
  )
}
