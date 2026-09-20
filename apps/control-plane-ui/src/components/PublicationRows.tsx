import type { LastPass, Publication } from '../api/publication-types'
import { Status } from '../console/Status'

/**
 * The read-only facts: what the target holds, and what the last pass did.
 *
 * Split out of `PublicationPanel` once the state, the button and these rows
 * together passed the file's line budget. These rows carry no state of
 * their own, so there is nothing here for `PublicationPanel.test.tsx` to
 * exercise separately — it renders the whole panel, the same as it would if
 * this were still inline.
 */
export function PublicationRows({ publication }: { publication: Publication }) {
  return (
    <dl className="facts">
      <dt>Target</dt>
      <dd>{publication.target}</dd>

      <dt>Tenants</dt>
      <dd>{revision(publication.documents.tenants)}</dd>

      <dt>Data sources</dt>
      <dd>{revision(publication.documents.dataSources)}</dd>

      <dt>Catalogue</dt>
      <dd>{revision(publication.documents.catalog)}</dd>

      <dt>Last pass</dt>
      <dd>
        <LastPassCell pass={publication.lastPass} />
      </dd>
    </dl>
  )
}

/**
 * `null` reads as "not reported", never `0` or "none" — see the doc comment
 * on `PublishedDocuments`. A revision is a fact the target holds right now;
 * there is no such thing as revision zero to fall back to.
 */
function revision(value: number | null): string {
  return value === null ? '—' : `revision ${String(value)}`
}

/**
 * `outcome`, capitalised for prose. Exported so `PublicationPanel`'s own
 * "Pass finished" notice reads a pass's outcome the same way this row's
 * `Status` pill does, rather than two places spelling the same word twice.
 */
export function capitalise(word: string): string {
  return word.charAt(0).toUpperCase() + word.slice(1)
}

/**
 * When the last pass finished, and what it did.
 *
 * Mirrors `PlatformPanel`'s own `LastCheck`: `Never` is the answer that
 * matters most, because it is the one that tells an operator nothing has
 * even been attempted yet, rather than attempted and refused.
 */
function LastPassCell({ pass }: { pass: LastPass | null }) {
  if (pass === null) {
    return <>Never</>
  }

  // Date and time, not just the time: this sits directly under the
  // environment card's own `Last check`, which renders both, and a pass
  // from three days ago must not read as though it happened today.
  const at = new Date(pass.atUnixSeconds * 1000).toLocaleString()

  return (
    <>
      {at} — <Status value={pass.outcome}>{capitalise(pass.outcome)}</Status>
      {pass.detail !== null && ` — ${pass.detail}`}
    </>
  )
}
