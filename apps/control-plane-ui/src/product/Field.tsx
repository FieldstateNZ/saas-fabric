/** What a {@link Field} needs: a label and a value always, the rest optional. */
interface FieldProps {
  readonly label: string
  readonly value: string
  readonly onChange: (value: string) => void
  readonly required?: boolean
  readonly type?: string
  readonly hint?: string
  /** A refusal specific to this field, such as an ID the server already has. Replaces `hint` when present. */
  readonly error?: string | null | undefined
}

/**
 * A single labelled text input, used across every product editor.
 *
 * `maxLength={4096}` is a client-side courtesy, not the validation: the
 * control plane enforces its own limits on every field. The browser applies
 * it as an operator types — a keystroke past the limit is silently
 * dropped — and to anything pasted in at once, cutting a longer paste down
 * to the first 4096 characters without asking. Either way it only ever
 * saves an operator from a refusal the control plane would send anyway; it
 * cannot warn them a paste was cut short.
 */
export function Field({
  label,
  value,
  onChange,
  required = false,
  type = 'text',
  hint,
  error,
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
        aria-invalid={error ? true : undefined}
        onChange={(event) => {
          onChange(event.target.value)
        }}
      />
      {error ? (
        <small className="field-error" role="alert">
          {error}
        </small>
      ) : (
        hint && <small>{hint}</small>
      )}
    </label>
  )
}
