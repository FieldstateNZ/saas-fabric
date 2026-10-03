import { useState } from 'react'

import { underHost } from '../../api/registries'
import type { Registry } from '../../api/registry-types'
import type { Refusal, RegistriesState } from '../../hooks/useRegistries'
import { Field } from '../../product/Field'
import { exampleUnder, when } from './registry-words'
import { RegistryRefusal } from './RegistryRefusal'

/**
 * A registry's repositories, each with when it was proven, and adding or
 * removing one.
 *
 * # Registered, not browsed
 *
 * Neither GHCR nor Docker Hub lists its repositories to an anonymous
 * client, so there is nothing to pick from: an operator types a repository
 * in full, as every image reference names it, and the control plane
 * records it only once its tag listing answers through this registry. One
 * that does not start with this registry's host is refused here, before
 * anything is sent, because it is not this registry's to hold.
 *
 * While nothing is read through the registry its repositories are listed
 * and cannot be changed: a change would be proven through a client the
 * control plane will not build.
 */
export function RegistryRepositories({
  registry,
  registries,
  changeable,
}: {
  readonly registry: Registry
  readonly registries: RegistriesState
  /** Whether repositories may be added or removed. */
  readonly changeable: boolean
}) {
  const [typed, setTyped] = useState('')
  const [problem, setProblem] = useState<string | null>(null)
  const [refusal, setRefusal] = useState<Refusal | null>(null)
  const [sending, setSending] = useState(false)

  async function add(): Promise<void> {
    setRefusal(null)
    const place = underHost(registry.host, typed.trim())

    if ('problem' in place) {
      setProblem(place.problem)
      return
    }

    setProblem(null)
    setSending(true)
    const refused = await registries.addRepository(registry.host, place.path)
    setSending(false)
    setRefusal(refused)

    if (refused === null) {
      setTyped('')
    }
  }

  async function remove(repository: string): Promise<void> {
    setRefusal(null)
    const place = underHost(registry.host, repository)

    setRefusal(
      'problem' in place
        ? { code: 'unexpected', message: place.problem }
        : await registries.removeRepository(registry.host, place.path),
    )
  }

  return (
    <div className="registry__part">
      <h4 className="registry__part-heading">Repositories</h4>

      {registry.repositories.length === 0 ? (
        <p className="integration__detail">None registered.</p>
      ) : (
        <ul className="registry__repositories">
          {registry.repositories.map((registered) => (
            <li key={registered.repository}>
              <span className="mono">{registered.repository}</span>
              <span className="registry__proven">proven {when(registered.provenAt)}</span>
              {changeable && (
                <button
                  type="button"
                  className="integration__action integration__action--quiet"
                  aria-label={`Remove ${registered.repository}`}
                  disabled={registries.busy}
                  onClick={() => void remove(registered.repository)}
                >
                  Remove
                </button>
              )}
            </li>
          ))}
        </ul>
      )}

      <RegistryRefusal refusal={refusal} />

      {changeable && (
        <form
          className="registry-form registry-form--inline"
          aria-label={`Add a repository to ${registry.host}`}
          onSubmit={(event) => {
            event.preventDefault()
            void add()
          }}
        >
          <fieldset disabled={registries.busy}>
            <Field
              label="Repository"
              required
              value={typed}
              error={problem}
              onChange={(value) => {
                setTyped(value)
                setProblem(null)
              }}
              hint={`Written in full, starting with ${registry.host}/ — for example ${exampleUnder(registry.host)}`}
            />
            <button type="submit" className="integration__action">
              {sending ? 'Proving…' : 'Add repository'}
            </button>
          </fieldset>
        </form>
      )}
    </div>
  )
}
