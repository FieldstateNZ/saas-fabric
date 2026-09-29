/**
 * How a described component's recorded digests are written on screen.
 * Moments are written by the registries section's `when`, so a time reads
 * the same wherever Fabric recorded it.
 */

/** Hex characters of a digest shown before it is cut short. */
const SHOWN = 12

/**
 * A digest cut short for reading, such as `sha256:1111111111…`. The full
 * value always goes beside it, as a title, so nothing is hidden -- only
 * shortened.
 */
export function shortDigest(digest: string): string {
  const [algorithm, hex] = digest.split(':', 2)
  return hex === undefined || hex.length <= SHOWN ? digest : `${algorithm ?? ''}:${hex.slice(0, SHOWN)}…`
}

