import { useState } from 'react'

import { describe } from '../../hooks/useClients'
import type { Secrets } from '../../hooks/useSecrets'

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

  async function submit(): Promise<void> {
    setBusy(true)
    onNotice(null)

    try {
      const expected = version.trim() === '' ? null : Number(version)

      await secrets.write(path.trim(), { [key.trim()]: value }, expected)

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
        value={version}
        placeholder="blank if new"
        onChange={(event) => {
          setVersion(event.target.value)
        }}
      />

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
