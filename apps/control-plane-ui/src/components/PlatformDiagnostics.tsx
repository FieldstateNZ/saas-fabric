import type { InvalidReasonCode, PlatformDiagnostic } from '../api/types'

/**
 * What each state is called. Keyed by the state itself, so a state the
 * control plane adds is a type error here rather than another's wording: an
 * operator deciding whether to wait must never read "built more than once"
 * about a version that is only still publishing, or either about one with no
 * component descriptor attached.
 */
const STATES: Readonly<Record<PlatformDiagnostic['state'], string>> = {
  publishing: 'still publishing',
  undescribed: 'no component descriptor attached',
  incoherent: 'built more than once',
  invalid: 'component descriptor cannot be used',
}

/**
 * Why a component descriptor cannot be used, in plain words: one for every
 * code, and only what Fabric observed. `notRegistered` is the catalogue's and
 * `notPinned` Platform Management's; both are here because the list is one.
 */
const REASONS: Readonly<Record<InvalidReasonCode, string>> = {
  unreadable: 'it, or the way it is attached, breaks the rules of its format',
  unsupportedVersion: 'it is written in a format version this Fabric does not read',
  wrongVersion: 'it names a different version from the tag it was found by',
  several: 'more than one is attached, or more are listed than Fabric checks',
  primaryNotNamed: 'it does not name the image it is attached to',
  otherRegistry: 'it names an image on another registry',
  missingImage: 'an image it names does not exist',
  noSingleRevision: 'an image, or the component descriptor itself, does not name exactly one commit',
  notRegistered: 'it names a repository that is not registered',
  notPinned: 'it names roles, repositories or a primary image other than the ones this environment pins',
}

/** One version that was not selected, in words. */
export function diagnosticWording(diagnostic: PlatformDiagnostic): string {
  const state = STATES[diagnostic.state]

  if (diagnostic.state !== 'invalid') {
    return state
  }
  // The one reason that names what Fabric found: the format version.
  if (diagnostic.reason === 'unsupportedVersion' && diagnostic.found) {
    return `${state}: it is written in format version ${diagnostic.found}, which this Fabric does not read`
  }

  return `${state}: ${REASONS[diagnostic.reason]}`
}

/**
 * The versions that exist and were not selected, and why.
 *
 * Nothing at all when there are none: an empty list is not a finding.
 */
export function PlatformDiagnostics({
  diagnostics,
}: {
  diagnostics: readonly PlatformDiagnostic[]
}) {
  if (diagnostics.length === 0) {
    return null
  }

  return (
    <ul className="platform__diagnostics">
      {diagnostics.map((diagnostic) => (
        <li key={diagnostic.version}>
          {diagnostic.version} — {diagnosticWording(diagnostic)}
        </li>
      ))}
    </ul>
  )
}
