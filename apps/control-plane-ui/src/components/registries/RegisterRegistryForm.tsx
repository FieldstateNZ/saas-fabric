import { useState } from 'react'

import type { RegistryKind, RegistryRegistration } from '../../api/registry-types'
import type { Refusal, RegistriesState } from '../../hooks/useRegistries'
import { Field } from '../../product/Field'
import { Select } from '../../product/Select'
import { CredentialFields } from './CredentialFields'
import { KIND_OPTIONS } from './registry-words'
import { RegistryRefusal } from './RegistryRefusal'

/**
 * Registers a registry: a kind, an endpoint only for `distribution`, and a
 * credential or none.
 *
 * There is no host field. `ghcr` and `dockerHub` are named by their kind,
 * and a `distribution` registry by its endpoint's host, so nothing typed
 * here can register `ghcr.io` as something else. Whether a username came
 * without a token, or an endpoint has a path, is the control plane's rule
 * to apply and its message to show; this form sends what was typed.
 */
export function RegisterRegistryForm({
  registries,
  onClose,
}: {
  readonly registries: RegistriesState
  readonly onClose: () => void
}) {
  const [kind, setKind] = useState<RegistryKind>('ghcr')
  const [endpoint, setEndpoint] = useState('')
  const [username, setUsername] = useState('')
  const [token, setToken] = useState('')
  const [refusal, setRefusal] = useState<Refusal | null>(null)
  const [sending, setSending] = useState(false)

  async function submit(): Promise<void> {
    setRefusal(null)
    setSending(true)
    const registration = registrationOf(kind, endpoint, username, token)
    // Out of the page before the answer, whatever it is (`CredentialFields`).
    setToken('')

    const refused = await registries.register(registration)
    setSending(false)

    if (refused === null) {
      onClose()
    } else {
      setRefusal(refused)
    }
  }

  return (
    <form
      className="integration registry-form"
      aria-label="Register a registry"
      onSubmit={(event) => {
        event.preventDefault()
        void submit()
      }}
    >
      <fieldset disabled={registries.busy}>
        <div className="form-grid">
          <Select
            label="Kind"
            required
            value={kind}
            options={KIND_OPTIONS}
            onChange={(value) => {
              const chosen = KIND_OPTIONS.find((option) => option.value === value)?.value
              if (chosen) {
                setKind(chosen)
              }
            }}
          />
          {kind === 'distribution' && (
            <Field
              label="Endpoint"
              required
              value={endpoint}
              onChange={setEndpoint}
              hint="An HTTPS origin, with a port if it has one: https://registry.example.com. No path."
            />
          )}
          <CredentialFields
            username={username}
            token={token}
            required={false}
            onUsername={setUsername}
            onToken={setToken}
          />
        </div>
        <p className="registry-form__note">
          A credential is optional: public repositories are read without one. Give both a username
          and a token, or neither.
        </p>
        <RegistryRefusal refusal={refusal} />
        <div className="form-actions">
          <button type="submit" className="primary-button">
            {sending ? 'Proving…' : 'Register'}
          </button>
          <button type="button" onClick={onClose}>
            Cancel
          </button>
        </div>
      </fieldset>
    </form>
  )
}

/** What is sent: only the fields this kind takes, and only the ones filled in. */
function registrationOf(
  kind: RegistryKind,
  endpoint: string,
  username: string,
  token: string,
): RegistryRegistration {
  return {
    kind,
    ...(kind === 'distribution' ? { endpoint: endpoint.trim() } : {}),
    ...(username.trim() === '' ? {} : { username: username.trim() }),
    ...(token.trim() === '' ? {} : { token: token.trim() }),
  }
}
