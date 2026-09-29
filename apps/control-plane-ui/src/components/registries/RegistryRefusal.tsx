import type { RegistryErrorCode } from '../../api/errors'
import type { Refusal } from '../../hooks/useRegistries'

/**
 * What an operator can take from each registry code, beyond the message.
 *
 * The message is always the control plane's own and shown as it came: it
 * names the rule, the host or the repository. What the code adds is the
 * consequence the message does not state -- chiefly that a change which
 * did not prove recorded nothing, because every registry change is proven
 * before it is recorded. A code this does not know adds nothing.
 */
const NEXT: Readonly<Record<RegistryErrorCode, string | null>> = {
  registry_not_found: 'The list has been read again.',
  registry_exists: 'Remove it before registering it again.',
  registry_invalid: null,
  registry_endpoint_differs: null,
  registry_credential_unreadable: 'Nothing was recorded.',
  registry_not_proven: 'Nothing was recorded.',
  repository_not_readable:
    'Nothing was recorded. A private repository is readable only with a credential that reaches it.',
  registry_refused: 'Nothing was recorded.',
  registry_unavailable: 'Nothing was recorded. Try again shortly.',
  registries_unavailable: 'Try again shortly.',
  registries_invalid: 'Trying again will not change this: the stored records need repairing.',
}

/** Whether `code` is one of the registry codes. */
function isRegistryCode(code: string): code is RegistryErrorCode {
  return Object.hasOwn(NEXT, code)
}

/** A registry change's refusal, beside the form that asked for it. */
export function RegistryRefusal({ refusal }: { readonly refusal: Refusal | null }) {
  if (refusal === null) {
    return null
  }

  const next = isRegistryCode(refusal.code) ? NEXT[refusal.code] : null

  return (
    <p className="error" role="alert">
      {refusal.message}
      {next !== null && (
        <>
          <br />
          {next}
        </>
      )}
    </p>
  )
}
