import type { ClientProductRequest } from '../api/catalogue-types'
import { Select } from './Select'

/** How a client's `.internal` hosts are reached: declared, never inferred (D01-15). */
type PrivateNetworkScheme = NonNullable<ClientProductRequest['privateNetworkScheme']>

/** Whether every one of `hosts` is under `.internal`, the private-use top-level domain. */
export function allInternal(hosts: readonly string[]): boolean {
  return hosts.length > 0 && hosts.every((host) => host === 'internal' || host.endsWith('.internal'))
}

/**
 * Asks how a client on `.internal` hosts is reached, when it is (D01-15).
 *
 * Shown only while every host is `.internal`, and never pre-filled: an
 * `.internal` name does not say whether its network terminates TLS, and the
 * identity provider matches a callback's scheme exactly, so a guess here is
 * a client nobody can sign in to. The control plane refuses a client on
 * `.internal` hosts with an application and no scheme, and refuses a scheme
 * on any other client; `ClientDetailsStep` clears it when the hosts stop
 * being `.internal`.
 */
export function PrivateNetworkSchemeSelect({
  hosts,
  value,
  onChange,
}: {
  readonly hosts: readonly string[]
  readonly value: PrivateNetworkScheme | null
  readonly onChange: (value: PrivateNetworkScheme | null) => void
}) {
  if (!allInternal(hosts)) {
    return null
  }

  return (
    <Select
      label="Callback scheme for .internal hosts"
      required
      value={value ?? ''}
      onChange={(chosen) => {
        onChange(chosen === 'http' || chosen === 'https' ? chosen : null)
      }}
      options={[
        { value: '', label: 'Choose how these hosts are reached…' },
        { value: 'http', label: 'http — the private network has no TLS listener' },
        { value: 'https', label: 'https — the private network terminates TLS' },
      ]}
    />
  )
}
