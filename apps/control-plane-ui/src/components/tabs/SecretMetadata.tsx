import { useRef, useState } from 'react'

import type { SecretMetadata as Metadata } from '../../api/types'
import { describe } from '../../hooks/useClients'

/** What the operator has been shown about one secret's version. */
type Observation =
  | { status: 'unread' }
  | { status: 'reading' }
  | { status: 'read'; version: number; updatedAt: string | null }
  | { status: 'failed'; error: string }

/**
 * The version a secret was last observed at, read only when asked.
 *
 * # Why this is a button and not a column
 *
 * Replacing a secret needs the version being replaced, and the listing does
 * not carry it: fetching it for every row would turn one cheap listing into a
 * read of the store per secret. So each row reads its version on demand, shows
 * what it observed, and the operator copies that into the write form. Nothing
 * is filled in for them -- the version they send is the one they chose.
 *
 * # What is shown is only ever the latest answer
 *
 * A read in flight clears the previous version and a failed read leaves none
 * on screen. An old number beside a fresh "Refresh" button would be an
 * invitation to send it. An answer that arrives after this row has gone has
 * nowhere to land: setting state on an unmounted component is a no-op.
 */
export function SecretMetadata({
  path,
  read,
}: {
  path: string
  read: (path: string) => Promise<Metadata>
}) {
  const [observation, setObservation] = useState<Observation>({ status: 'unread' })
  // A ref rather than `observation.status`: two clicks in one tick would both
  // see the state from before either rendered. It also means at most one read
  // is ever in flight for this row, so no answer can be superseded by another.
  const pending = useRef(false)

  function observe(): void {
    if (pending.current) {
      return
    }

    pending.current = true
    setObservation({ status: 'reading' })

    const settle = (answer: Observation): void => {
      pending.current = false
      setObservation(answer)
    }

    // The answer is taken as `unknown`, whatever `read` claims: the type is a
    // promise about the wire, and a `200` whose body is `null` keeps the type
    // and breaks the promise. `fromMetadata` is total over `unknown`, so there
    // is nothing left in the fulfilment branch that can throw past `settle` --
    // which is what would leave this row reading forever.
    read(path).then(
      (metadata: unknown) => {
        settle(fromMetadata(metadata))
      },
      (thrown: unknown) => {
        settle({ status: 'failed', error: describe(thrown) })
      },
    )
  }

  // `role="status"` carries both the reading and the observed result, so a
  // screen reader hears the outcome and not only that a read began. Its
  // element is always in the tree: a polite live region is announced when its
  // content changes, not when it is inserted already filled. `role="alert"`
  // is the exception -- it is announced on insertion -- so the failure needs
  // no persistent container.
  const { text, name } = describeButton(observation, path)

  return (
    <span>
      <span role="status">
        {observation.status === 'reading' && 'Reading version…'}
        {observation.status === 'read' && (
          <>
            Observed version <code>{observation.version}</code> of {path}
            {observation.updatedAt !== null && <> (updated {observation.updatedAt})</>}
          </>
        )}
      </span>
      {observation.status === 'failed' && (
        <span className="error" role="alert">
          {observation.error}
        </span>
      )}

      <button
        type="button"
        aria-label={name}
        disabled={observation.status === 'reading'}
        onClick={observe}
      >
        {text}
      </button>
    </span>
  )
}

/** What a failed read says when the answer was not one this console can use. */
const UNUSABLE: Observation = {
  status: 'failed',
  error: 'The control plane answered with a version this console cannot carry.',
}

/**
 * Accepts only a version this console can send back unchanged.
 *
 * Takes the body as `unknown` and never throws: the shape is checked before
 * anything is read from it, so `null`, an array, a bare string or number, or
 * an object with no `version` all become the same retryable failure rather
 * than an exception nobody is waiting for.
 *
 * The store's version is a `u64`; this console carries it as a JavaScript
 * number, which is exact only up to 2^53 - 1. A version beyond that, or one
 * that is not a non-negative integer at all, would be shown as something other
 * than what the store holds -- so it is shown as an error instead.
 */
function fromMetadata(metadata: unknown): Observation {
  if (typeof metadata !== 'object' || metadata === null || Array.isArray(metadata)) {
    return UNUSABLE
  }

  const version = 'version' in metadata ? metadata.version : undefined
  const updatedAt = 'updatedAt' in metadata ? metadata.updatedAt : undefined

  if (typeof version !== 'number' || !Number.isSafeInteger(version) || version < 0) {
    return UNUSABLE
  }

  return { status: 'read', version, updatedAt: typeof updatedAt === 'string' ? updatedAt : null }
}

/**
 * The button's visible text, and its accessible name: the same words with the
 * secret they act on, since every row's button otherwise reads identically.
 */
function describeButton(
  observation: Observation,
  path: string,
): { readonly text: string; readonly name: string } {
  switch (observation.status) {
    case 'unread':
      return { text: 'Read version', name: `Read version of ${path}` }
    case 'reading':
      return { text: 'Reading…', name: `Reading version of ${path}` }
    case 'read':
      return { text: 'Refresh version', name: `Refresh version of ${path}` }
    case 'failed':
      return { text: 'Retry', name: `Retry reading version of ${path}` }
  }
}
