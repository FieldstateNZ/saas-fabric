import type { ClientProductRequest, ConfigurationField } from '../api/catalogue-types'
import { ConfigurationInputs } from './ConfigurationInputs'
import { Field } from './Field'

/** What the client-details step of `ClientForm` needs to read and edit it. */
interface ClientDetailsStepProps {
  readonly id: string
  readonly onIdChange: (id: string) => void
  readonly existing: boolean
  readonly value: ClientProductRequest
  readonly onValueChange: (value: ClientProductRequest) => void
  readonly hostText: string
  readonly onHostTextChange: (text: string) => void
  readonly clientFields: readonly ConfigurationField[]
}

/**
 * The first step of {@link ClientForm}: the client's identity and its own
 * configuration fields.
 *
 * `hostText` is kept as raw, unsplit text rather than deriving it from
 * `value.hosts` on every render — an operator mid-way through typing "acme,
 * ex" has not yet produced a second hostname, and re-deriving from the split
 * array would either drop the trailing fragment or fight their cursor.
 * `id` is only editable when there is no `existing` client: a client's ID is
 * permanent once created.
 */
export function ClientDetailsStep({
  id,
  onIdChange,
  existing,
  value,
  onValueChange,
  hostText,
  onHostTextChange,
  clientFields,
}: ClientDetailsStepProps) {
  return (
    <div className="panel panel-body">
      <div className="form-grid">
        {!existing && (
          <Field
            label="Client ID"
            required
            value={id}
            onChange={onIdChange}
            hint="Permanent identifier: lowercase letters, numbers and hyphens."
          />
        )}
        <Field
          label="Display name"
          required
          value={value.displayName}
          onChange={(displayName) => {
            onValueChange({ ...value, displayName })
          }}
        />
        <Field
          label="Legal name"
          required
          value={value.legalName}
          onChange={(legalName) => {
            onValueChange({ ...value, legalName })
          }}
        />
        <Field
          label="Region"
          required
          value={value.region}
          onChange={(region) => {
            onValueChange({ ...value, region })
          }}
        />
        <Field
          label="Timezone"
          required
          value={value.timezone}
          onChange={(timezone) => {
            onValueChange({ ...value, timezone })
          }}
        />
        <Field
          label="Hostnames (comma separated)"
          value={hostText}
          onChange={(hosts) => {
            onHostTextChange(hosts)
            onValueChange({
              ...value,
              // Commas, whitespace and newlines all separate hostnames — a
              // pasted list is as likely to be one per line, or space
              // separated, as it is comma separated.
              hosts: hosts
                .split(/[\s,]+/)
                .map((h) => h.trim())
                .filter(Boolean),
            })
          }}
        />
      </div>
      <ConfigurationInputs
        fields={clientFields}
        values={value.configuration}
        onChange={(configuration) => {
          onValueChange({ ...value, configuration })
        }}
      />
    </div>
  )
}
