import { useState } from 'react'

import type { Refusal, RegistriesState } from '../../hooks/useRegistries'
import { RegistryRefusal } from './RegistryRefusal'

/**
 * Removing a registry, after saying what goes with it.
 *
 * A two-step confirm, as `DataSourceRow`'s is: the first click only asks.
 * Removal takes the credential and every registered repository with it, and
 * a token cannot be read back to restore it -- so the question names both.
 * What a catalogue already recorded from this registry stays recorded; only
 * choosing another version needs the registry back (ADR 0026 section 5).
 */
export function RemoveRegistry({
  host,
  held,
  registries,
}: {
  readonly host: string
  /** Whether it holds a credential, which cannot be read back once removed. */
  readonly held: boolean
  readonly registries: RegistriesState
}) {
  const [confirming, setConfirming] = useState(false)
  const [refusal, setRefusal] = useState<Refusal | null>(null)

  async function remove(): Promise<void> {
    setRefusal(null)
    const refused = await registries.remove(host)

    // A success unmounts this card; only a refusal is left to show.
    setConfirming(false)
    setRefusal(refused)
  }

  return (
    <>
      <RegistryRefusal refusal={refusal} />

      {confirming ? (
        <div className="registry__confirm" role="group" aria-label={`Remove ${host}`}>
          <p>
            {held
              ? `Remove ${host}? Its credential and its registered repositories go with it; registering it again means typing the token again.`
              : `Remove ${host}? Its registered repositories go with it.`}
          </p>
          <div className="integration__actions">
            <button
              type="button"
              className="integration__action"
              disabled={registries.busy}
              onClick={() => void remove()}
            >
              Remove registry
            </button>
            <button
              type="button"
              className="integration__action integration__action--quiet"
              disabled={registries.busy}
              onClick={() => {
                setConfirming(false)
              }}
            >
              Keep it
            </button>
          </div>
        </div>
      ) : (
        <div className="integration__actions">
          <button
            type="button"
            className="integration__action integration__action--quiet"
            disabled={registries.busy}
            onClick={() => {
              setRefusal(null)
              setConfirming(true)
            }}
          >
            Remove {host}…
          </button>
        </div>
      )}
    </>
  )
}
