import type { RegistryVersions } from '../../api/registry-types'
import { Select } from '../Select'
import type { Read } from './usePickerSources'

/** How many tags were left out, said so the list does not seem to have missed them. */
function leftOut(other: number): string | null {
  if (other === 0) {
    return null
  }
  return other === 1
    ? '1 tag is not a version and is not listed.'
    : `${String(other)} tags are not versions and are not listed.`
}

/**
 * A repository's version tags, newest first as the control plane lists
 * them, and how many of its tags were not versions.
 */
export function VersionChoice({
  versions,
  value,
  onChange,
}: {
  readonly versions: Read<RegistryVersions>
  readonly value: string
  readonly onChange: (version: string) => void
}) {
  if (versions.loading) {
    return <p className="empty">Listing version tags…</p>
  }
  if (versions.error !== null) {
    return (
      <p className="field-error" role="alert">
        The version tags could not be listed: {versions.error}
      </p>
    )
  }
  if (versions.value === null) {
    return null
  }

  const note = leftOut(versions.value.other)
  return (
    <>
      {versions.value.tags.length === 0 ? (
        <p className="empty">No tag of this repository is a version.</p>
      ) : (
        <Select
          label="Version"
          value={value}
          options={[
            { value: '', label: 'Choose a version…' },
            ...versions.value.tags.map((tag) => ({ value: tag, label: tag })),
          ]}
          onChange={onChange}
        />
      )}
      {note !== null && <p className="support-note">{note}</p>}
    </>
  )
}
