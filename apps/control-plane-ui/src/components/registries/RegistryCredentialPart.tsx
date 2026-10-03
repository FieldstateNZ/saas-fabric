import { useState } from 'react'

import type { Registry } from '../../api/registry-types'
import type { Refusal, RegistriesState } from '../../hooks/useRegistries'
import { CredentialFacts } from './CredentialFacts'
import { CredentialFields } from './CredentialFields'
import { RegistryRefusal } from './RegistryRefusal'

/**
 * A registry's credential: what is held, and setting, replacing or
 * removing it.
 *
 * Setting or replacing one is proven -- the registry and every repository
 * registered under it must answer with it -- before the control plane
 * records it, so the button says so while it waits. Removing one is not
 * proven: a credential must always be withdrawable, even from a registry
 * whose private repositories will stop being readable without it -- or one
 * nothing is read through, which is why removing stays offered when
 * setting does not.
 */
export function RegistryCredentialPart({
  registry,
  registries,
  settable,
}: {
  readonly registry: Registry
  readonly registries: RegistriesState
  /** Whether a credential may be set here: not while nothing is read through the registry. */
  readonly settable: boolean
}) {
  const [editing, setEditing] = useState(false)
  const [username, setUsername] = useState('')
  const [token, setToken] = useState('')
  const [refusal, setRefusal] = useState<Refusal | null>(null)
  const [sending, setSending] = useState(false)
  const held = registry.credential

  async function save(): Promise<void> {
    setRefusal(null)
    setSending(true)
    const credential = { username: username.trim(), token: token.trim() }
    // Out of the page before the answer, whatever it is (`CredentialFields`).
    setToken('')

    const refused = await registries.setCredential(registry.host, credential)
    setSending(false)
    setRefusal(refused)

    if (refused === null) {
      setEditing(false)
      setUsername('')
    }
  }

  async function remove(): Promise<void> {
    setRefusal(null)
    setRefusal(await registries.removeCredential(registry.host))
  }

  return (
    <div className="registry__part">
      <h4 className="registry__part-heading">Credential</h4>

      {held === null ? (
        <p className="integration__detail">None. This registry is read anonymously.</p>
      ) : (
        <CredentialFacts registry={registry} credential={held} />
      )}

      <RegistryRefusal refusal={refusal} />

      {editing ? (
        <form
          className="registry-form"
          aria-label={`Credential for ${registry.host}`}
          onSubmit={(event) => {
            event.preventDefault()
            void save()
          }}
        >
          <fieldset disabled={registries.busy}>
            <div className="form-grid">
              <CredentialFields
                username={username}
                token={token}
                required
                onUsername={setUsername}
                onToken={setToken}
              />
            </div>
            <div className="form-actions">
              <button type="submit" className="primary-button">
                {sending ? 'Proving…' : 'Save credential'}
              </button>
              <button
                type="button"
                onClick={() => {
                  setEditing(false)
                  setToken('')
                }}
              >
                Cancel
              </button>
            </div>
          </fieldset>
        </form>
      ) : (
        <div className="integration__actions">
          {settable && (
            <button
              type="button"
              className="integration__action"
              disabled={registries.busy}
              onClick={() => {
                setRefusal(null)
                setEditing(true)
              }}
            >
              {held === null ? 'Set credential' : 'Replace credential'}
            </button>
          )}
          {held !== null && (
            <button
              type="button"
              className="integration__action integration__action--quiet"
              disabled={registries.busy}
              onClick={() => void remove()}
            >
              Remove credential
            </button>
          )}
        </div>
      )}
    </div>
  )
}
