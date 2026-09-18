import type { EnvironmentRegistration } from '../api/catalogue-types'
import { Field } from './Field'

/**
 * Registering, or correcting, a link to another independently authenticated
 * operator console.
 *
 * Split out of `Environments` purely for size -- see that file's own
 * comment on the line band it sits in. There is no state here of its own:
 * `environment`, `onChange`, `onSubmit`, `onCancel` and `saving` all belong
 * to `Environments`, passed straight through.
 */
export function EnvironmentRegistrationForm({
  environment,
  onChange,
  onSubmit,
  onCancel,
  saving,
}: {
  environment: EnvironmentRegistration
  onChange: (environment: EnvironmentRegistration) => void
  onSubmit: () => void
  onCancel: () => void
  saving: boolean
}) {
  return (
    <form
      className="panel panel-body"
      onSubmit={(event) => {
        event.preventDefault()
        onSubmit()
      }}
    >
      <fieldset disabled={saving}>
        <div className="form-grid">
          <Field
            label="Environment ID"
            required
            value={environment.id}
            onChange={(id) => {
              onChange({ ...environment, id })
            }}
          />
          <Field
            label="Environment name"
            required
            value={environment.name}
            onChange={(name) => {
              onChange({ ...environment, name })
            }}
          />
          <Field
            label="Console URL"
            required
            type="url"
            value={environment.consoleUrl}
            onChange={(consoleUrl) => {
              onChange({ ...environment, consoleUrl })
            }}
            hint="An HTTPS operator-console origin, such as https://console.example.com."
          />
          <Field
            label="Description"
            value={environment.description}
            onChange={(description) => {
              onChange({ ...environment, description })
            }}
          />
        </div>
        <div className="form-actions">
          <button className="primary-button" type="submit">
            Save environment
          </button>
          <button
            type="button"
            onClick={onCancel}
          >
            Cancel
          </button>
        </div>
      </fieldset>
    </form>
  )
}
