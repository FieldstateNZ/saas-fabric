import { Field } from '../product/Field'
import { Select } from '../product/Select'

/**
 * How a data source's connector reaches it: a name the connector already
 * holds, or a reference to a secret. Never a value -- ADR 0023 refuses to
 * let a credential be declared here, and `ConnectionSelector`'s two variants
 * are the only ones that exist.
 */
export function ConnectionFields({
  kind,
  value,
  onChange,
}: {
  kind: 'named' | 'secret'
  value: string
  onChange: (kind: 'named' | 'secret', value: string) => void
}) {
  return (
    <div className="form-grid">
      <Select
        label="Connection"
        required
        value={kind}
        options={[
          { value: 'named', label: 'Named connection' },
          { value: 'secret', label: 'Secret reference' },
        ]}
        onChange={(next) => {
          onChange(next === 'secret' ? 'secret' : 'named', value)
        }}
      />
      <Field
        label={kind === 'named' ? 'Connection name' : 'Secret reference'}
        required
        value={value}
        onChange={(next) => {
          onChange(kind, next)
        }}
        hint={
          kind === 'named'
            ? 'The name the connector already holds this connection under.'
            : 'A path to the secret, never a value.'
        }
      />
    </div>
  )
}
