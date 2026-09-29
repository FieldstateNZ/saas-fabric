import type { Registry, RegistryCredential } from '../../api/registry-types'
import { when } from './registry-words'

/**
 * What is known about a registry's credential: whose account, who set it
 * and when, and what became of it. Never the token, which the control
 * plane does not return.
 *
 * # Only what was observed
 *
 * `refused` and `unreadable` are the control plane's observations and are
 * said as such. Otherwise this states the rule the credential is presented
 * under -- only for the repositories registered under this registry -- and
 * not that it works: nothing has observed that since it was last proven.
 */
export function CredentialFacts({
  registry,
  credential,
}: {
  readonly registry: Registry
  readonly credential: RegistryCredential
}) {
  return (
    <>
      <dl className="integration__rows">
        <dt>Username</dt>
        <dd>{credential.username}</dd>
        <dt>Set</dt>
        <dd>
          by {credential.setBy}, {when(credential.setAt)}
        </dd>
      </dl>
      <CredentialStanding registry={registry} credential={credential} />
    </>
  )
}

/** The one sentence about how the credential stands now. */
function CredentialStanding({
  registry,
  credential,
}: {
  readonly registry: Registry
  readonly credential: RegistryCredential
}) {
  if (credential.refused) {
    return (
      <p className="integration__diagnostic">
        Refused by its realm. It is not presented again until it is replaced.
      </p>
    )
  }

  if (credential.unreadable) {
    return (
      <p className="integration__diagnostic">
        Could not be read when the control plane last started, so this registry is read
        anonymously until the credential is set again.
      </p>
    )
  }

  if (!registry.installed) {
    return null
  }

  return (
    <p className="integration__detail">
      Presented only for the repositories registered below. Every other repository on{' '}
      {registry.host} is read anonymously.
    </p>
  )
}
