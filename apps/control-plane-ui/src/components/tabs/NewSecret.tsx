// Size: 121 code lines, the floor of the 121–150 band in
// docs/architecture/file-size-policy.md. This is one form and the one check
// that guards its one dangerous field. The parser is kept beside the form
// because its only caller is the submit handler, and its refusal text is the
// form's accessible error; moving it out would separate the rule from the
// field it describes without anything else ever importing it.
import { useRef, useState } from 'react'
import { flushSync } from 'react-dom'

import { describe } from '../../hooks/useClients'
import type { Secrets } from '../../hooks/useSecrets'

/** How a "Replacing version" field reads: a version, a blank, or a refusal. */
type ParsedVersion =
  | { readonly ok: true; readonly expected: number | null }
  | { readonly ok: false; readonly reason: string }

/**
 * Reads the "Replacing version" field. Blank means "new" (`null`); anything
 * else must be decimal digits spelling a safe integer. Zero is accepted
 * because it is well-formed, but it is not a first stored version: the server
 * passes it to the store as the check-and-set value, and a check-and-set of
 * zero means "write only if nothing is stored yet" — the same as blank.
 *
 * # Why the text is inspected, not the number
 *
 * `JSON.stringify` turns `NaN` and `Infinity` into `null` — the very spelling
 * of "this does not exist yet" — and rounds integers past 2^53 to a neighbour.
 * But `Number()` itself is lossy before any of that: `'9007199254740991.1'`
 * rounds to `9007199254740991`, `'1.0000000000000001'` to `1`, and `'1e-999'`
 * underflows to `0` — a create-only write. Each passes `isSafeInteger`, so a
 * typo would reach the control plane as a different, well-formed request. The
 * only reliable check is on what was typed: digits, then the safe range.
 */
function parseExpectedVersion(raw: string): ParsedVersion {
  const trimmed = raw.trim()

  if (trimmed === '') {
    return { ok: true, expected: null }
  }

  // Digits only: no sign, fraction, exponent, hex or `Infinity` can slip in.
  if (!/^[0-9]+$/.test(trimmed) || !Number.isSafeInteger(Number(trimmed))) {
    return {
      ok: false,
      reason:
        'Replacing version must be a whole number written in decimal digits, from 0 to 9007199254740991, or blank for a new secret.',
    }
  }

  return { ok: true, expected: Number(trimmed) }
}

/**
 * Creating or replacing one secret.
 *
 * # Why one form does both
 *
 * The store has no separate create: a write is a write, and what distinguishes
 * them is the version the operator believes they are replacing. Leaving the
 * version blank says "I believe this does not exist yet", which is what makes
 * an accidental overwrite a refusal rather than a silent replacement.
 */
export function NewSecret({
  secrets,
  onNotice,
}: {
  secrets: Secrets
  onNotice: (notice: string | null) => void
}) {
  const [path, setPath] = useState('')
  const [key, setKey] = useState('')
  const [value, setValue] = useState('')
  const [version, setVersion] = useState('')
  const [busy, setBusy] = useState(false)
  const [versionError, setVersionError] = useState<string | null>(null)
  const versionInput = useRef<HTMLInputElement>(null)

  async function submit(): Promise<void> {
    onNotice(null)

    // Refused before anything is sent, and the fields are left as typed so the
    // operator can correct the one that was wrong.
    const parsed = parseExpectedVersion(version)
    if (!parsed.ok) {
      flushSync(() => {
        setVersionError(parsed.reason)
      })
      versionInput.current?.focus()
      return
    }

    setVersionError(null)
    setBusy(true)

    try {
      await secrets.write(path.trim(), { [key.trim()]: value }, parsed.expected)

      onNotice(`Wrote ${path.trim()}.`)
      setPath('')
      setKey('')
      setValue('')
      setVersion('')
    } catch (thrown: unknown) {
      onNotice(describe(thrown))
    } finally {
      setBusy(false)
    }
  }

  return (
    <form
      className="secrets__new"
      onSubmit={(event) => {
        event.preventDefault()
        void submit()
      }}
    >
      <h3>Write a secret</h3>

      <label htmlFor="secret-path">Path</label>
      <input
        id="secret-path"
        value={path}
        placeholder="database/primary"
        onChange={(event) => {
          setPath(event.target.value)
        }}
      />

      <label htmlFor="secret-key">Key</label>
      <input
        id="secret-key"
        value={key}
        placeholder="password"
        onChange={(event) => {
          setKey(event.target.value)
        }}
      />

      <label htmlFor="secret-value">Value</label>
      <input
        id="secret-value"
        type="password"
        value={value}
        onChange={(event) => {
          setValue(event.target.value)
        }}
      />

      <label htmlFor="secret-version">Replacing version</label>
      <input
        id="secret-version"
        ref={versionInput}
        value={version}
        placeholder="blank if new"
        aria-invalid={versionError !== null ? true : undefined}
        aria-describedby={versionError !== null ? 'secret-version-error' : undefined}
        onChange={(event) => {
          setVersion(event.target.value)
          setVersionError(null)
        }}
      />
      {versionError !== null && (
        <p id="secret-version-error" className="error" role="alert">
          {versionError}
        </p>
      )}

      <button
        type="submit"
        className="signin__button"
        disabled={busy || path.trim() === '' || key.trim() === ''}
      >
        {busy ? 'Writing…' : 'Write'}
      </button>
    </form>
  )
}
