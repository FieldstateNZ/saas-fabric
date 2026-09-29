/**
 * The plain words for what the release-unit rule answered (ADR 0026 section
 * 3), shared by the platform's diagnostics and the catalogue's component
 * picker, so one answer is worded one way wherever an operator meets it.
 */
import type { InvalidReasonCode, PlatformDiagnostic } from '../api/types'

/**
 * What each state is called. Keyed by the state itself, so a state the
 * control plane adds is a type error here rather than another's wording: an
 * operator deciding whether to wait must never read "built more than once"
 * about a version that is only still publishing, or either about one with no
 * component descriptor attached.
 */
export const STATES: Readonly<Record<PlatformDiagnostic['state'], string>> = {
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
export const REASONS: Readonly<Record<InvalidReasonCode, string>> = {
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
