import type { PlatformDiagnostic } from '../api/types'
import { REASONS, STATES } from './diagnostic-words'

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
