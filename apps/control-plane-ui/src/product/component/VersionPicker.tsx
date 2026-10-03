import { useState } from 'react'

import type { ControlPlaneError } from '../../api/errors'
import { Select } from '../Select'
import { PickerRefusal } from './PickerRefusal'
import { useRegistryListing, useVersions } from './usePickerSources'
import { VersionChoice } from './VersionChoice'

/** What a {@link VersionPicker} needs from the component it selects for. */
export interface VersionPickerProps {
  /** A repository to start from: the one a described component already records. */
  readonly initialRepository?: string
  /** Why selecting is not possible right now, said beside the disabled button; `null` when it is. */
  readonly blocked: string | null
  /** Sends the selection; `null` once the answer replaced the draft, or the refusal. */
  readonly select: (repository: string, version: string) => Promise<ControlPlaneError | null>
  readonly onSelected: () => void
  readonly onCancel?: () => void
  readonly onReload: () => void
}

/**
 * Choosing a component artifact: a registry, one of its registered
 * repositories, and one of that repository's version tags (ADR 0026 section
 * 7).
 *
 * # The list is navigation; the selection is validation
 *
 * Only what is registered is offered -- there is no field to type a
 * repository into -- and the versions are the tags that parse as one,
 * newest first. Nothing listed is claimed to be a release unit: "Use this
 * version" sends the repository and the tag, never a digest, and the
 * server applies the rule. A refusal is shown in plain words ahead of the
 * control plane's own message.
 *
 * In the 121-150 line band: the three choices depend on one another -- a
 * registry resets its repository, a repository its version -- and the
 * send and its refusal read all three, so they are one component's state.
 * The version list and the reads are already split out.
 *
 * A registry the control plane is not reading through -- recorded, and not
 * rebuilt when it last started -- is offered marked as such, and nothing is
 * listed or selected from it: its answer would be about a client that does
 * not exist, not about the registry.
 *
 * # Disabled while the draft has unsaved changes
 *
 * Selecting writes the saved draft, so it would lose every unsaved change
 * beside it; `blocked` says so rather than leaving a dead button.
 */
export function VersionPicker({
  initialRepository = '',
  blocked,
  select,
  onSelected,
  onCancel,
  onReload,
}: VersionPickerProps) {
  const listing = useRegistryListing()
  const [host, setHost] = useState(initialRepository.split('/', 1)[0] ?? '')
  const [repository, setRepository] = useState(initialRepository)
  const [version, setVersion] = useState('')
  const [sending, setSending] = useState(false)
  const [refusal, setRefusal] = useState<ControlPlaneError | null>(null)

  const registries = listing.value?.registries ?? []
  const registry = registries.find((held) => held.host === host)
  const unread = registry?.installed === false
  const repositories = unread ? [] : (registry?.repositories.map((held) => held.repository) ?? [])
  const chosen = repositories.includes(repository) ? repository : ''
  const versions = useVersions(registry === undefined ? '' : host, chosen)

  async function use() {
    setSending(true)
    setRefusal(null)
    const refused = await select(chosen, version)
    setSending(false)
    if (refused === null) {
      onSelected()
    } else {
      setRefusal(refused)
    }
  }

  if (listing.loading) {
    return <p className="empty picker">Loading registries…</p>
  }
  if (listing.error !== null) {
    return (
      <p className="field-error picker" role="alert">
        The registries could not be read: {listing.error}
      </p>
    )
  }

  return (
    <section className="picker" aria-label="Choose a component artifact">
      {registries.length === 0 ? (
        <p className="empty">
          No registries are registered. Register one, and its repositories, under Integrations.
        </p>
      ) : (
        <Select
          label="Registry"
          value={registry === undefined ? '' : host}
          options={[
            { value: '', label: 'Choose a registry…' },
            ...registries.map((held) => ({
              value: held.host,
              label: held.installed ? held.host : `${held.host} (not read through since the last restart)`,
            })),
          ]}
          onChange={(next) => {
            setHost(next)
            setRepository('')
            setVersion('')
            setRefusal(null)
          }}
        />
      )}
      {unread && (
        <p className="empty">
          Nothing is read through {registry.host} since the control plane last started, so no
          version can be selected from it. Its log says why.
        </p>
      )}
      {registry !== undefined && !unread && repositories.length === 0 && (
        <p className="empty">No repositories are registered under {registry.host}.</p>
      )}
      {repositories.length > 0 && (
        <Select
          label="Repository"
          value={chosen}
          options={[
            { value: '', label: 'Choose a repository…' },
            ...repositories.map((name) => ({ value: name, label: name })),
          ]}
          onChange={(next) => {
            setRepository(next)
            setVersion('')
            setRefusal(null)
          }}
        />
      )}
      {chosen !== '' && (
        <VersionChoice
          versions={versions}
          value={version}
          onChange={(next) => {
            setVersion(next)
            setRefusal(null)
          }}
        />
      )}

      <div className="form-actions">
        <button
          type="button"
          className="primary-button"
          disabled={blocked !== null || chosen === '' || version === '' || sending}
          onClick={() => {
            void use()
          }}
        >
          {sending ? 'Selecting…' : 'Use this version'}
        </button>
        {onCancel && (
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        )}
      </div>
      {blocked !== null && <p className="support-note">{blocked}</p>}

      {refusal !== null && (
        <PickerRefusal
          refusal={refusal}
          onReload={() => {
            setRefusal(null)
            onReload()
          }}
        />
      )}
    </section>
  )
}
