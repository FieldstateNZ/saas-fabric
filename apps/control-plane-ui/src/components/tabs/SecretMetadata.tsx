import { useEffect, useRef, useState } from 'react'

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
 * A read in flight clears the previous version, a failed read leaves none on
 * screen, and an answer that arrives after this row has gone is dropped. An
 * old number beside a fresh "Refresh" button would be an invitation to send it.
 */
export function SecretMetadata({
  path,
  read,
}: {
  path: string
  read: (path: string) => Promise<Metadata>
}) {
  const [observation, setObservation] = useState<Observation>({ status: 'unread' })
  // Which read is current. A response for any other number is obsolete: it
  // was either superseded by a later read or outlived the row that asked.
  const current = useRef(0)
  // A ref rather than `observation.status`: two clicks in one tick would both
  // see the state from before either rendered.
  const pending = useRef(false)

  useEffect(
    () => () => {
      current.current += 1
    },
    [],
  )

  function observe(): void {
    if (pending.current) {
      return
    }

    const sequence = current.current + 1
    current.current = sequence
    pending.current = true
    setObservation({ status: 'reading' })

    const settle = (answer: Observation): void => {
      pending.current = false
      if (current.current === sequence) {
        setObservation(answer)
      }
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

  // `role="status"` while reading and `role="alert"` on failure, as the rest
  // of the console does for the same two states: the text changes under a
  // button the operator just pressed, and a screen reader is told about it.
  return (
    <span>
      {observation.status === 'reading' && <span role="status">Reading version…</span>}
      {observation.status === 'failed' && (
        <span className="error" role="alert">
          {observation.error}
        </span>
      )}
      {observation.status === 'read' && (
        <span>
          Observed version <code>{observation.version}</code>
          {observation.updatedAt !== null && <> (updated {observation.updatedAt})</>}
        </span>
      )}

      <button type="button" disabled={observation.status === 'reading'} onClick={observe}>
        {label(observation)}
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

function label(observation: Observation): string {
  switch (observation.status) {
    case 'unread':
      return 'Read version'
    case 'reading':
      return 'Reading…'
    case 'read':
      return 'Refresh version'
    case 'failed':
      return 'Retry'
  }
}
