import { useId, useState } from 'react'

import type { IntegrationEndpoints } from '../api/client'
import type { Application } from '../api/types'
import { describe } from '../hooks/useClients'

/**
 * Creating and installing one of this platform's applications.
 *
 * # Two approvals, so two buttons
 *
 * Creating the application on the Git host and installing it are separate
 * approvals there, and an operator can complete the first and abandon the
 * second. The console shows whichever step is outstanding rather than
 * pretending it is one action that sometimes half-works.
 *
 * # It is handed its endpoints, and cannot reach the others
 *
 * Which application this creates is decided by whoever renders it. There is no
 * kind to pass and no default to fall back on, so there is no path by which
 * the platform panel could create the client application or the reverse.
 *
 * The organisation field is the one free-text input in either flow, and it is
 * not a repository: GitHub creates an application *in* an organisation, and
 * that is the only thing it names. Repositories are never typed — they are
 * picked from what the installation reaches.
 */
interface ConnectApplicationProps {
  readonly endpoints: IntegrationEndpoints

  /** The application so far, or `null` when there is not one yet. */
  readonly application: Application | null

  /** What this application is for, in the operator's words. */
  readonly purpose: string
}

export function ConnectApplication({ endpoints, application, purpose }: ConnectApplicationProps) {
  // Both panels can render this at once, so the label must point at *this*
  // field rather than at whichever one the browser found first.
  const field = useId()
  const [organisation, setOrganisation] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function connect(): Promise<void> {
    setBusy(true)
    setError(null)

    try {
      const { post_url, manifest } = await endpoints.beginConnection(organisation.trim())
      postManifest(post_url, manifest)
    } catch (thrown: unknown) {
      setError(describe(thrown))
      setBusy(false)
    }
    // On success the page navigates away, so `busy` is deliberately left set.
  }

  async function install(): Promise<void> {
    setBusy(true)
    setError(null)

    try {
      const { url } = await endpoints.beginInstall()
      window.location.assign(url)
    } catch (thrown: unknown) {
      setError(describe(thrown))
      setBusy(false)
    }
  }

  const created = application !== null

  return (
    <div className="connect">
      {error !== null && <p className="error">{error}</p>}

      {created ? (
        <>
          <p className="connect__lead">
            SaaS Fabric has an application on GitHub. Install it on the organisation that holds{' '}
            {purpose}.
          </p>
          <button type="button" className="signin__button" onClick={() => void install()} disabled={busy}>
            {busy ? 'Opening GitHub…' : 'Install on GitHub'}
          </button>
        </>
      ) : (
        <>
          <p className="connect__lead">
            SaaS Fabric will create its own GitHub application for {purpose}. You approve it on
            GitHub &mdash; there is no application to make by hand and no key to copy.
          </p>
          <label className="connect__label" htmlFor={field}>
            GitHub organisation
          </label>
          <input
            id={field}
            className="connect__input"
            value={organisation}
            onChange={(event) => {
              setOrganisation(event.target.value)
            }}
            placeholder="your-organisation"
            disabled={busy}
          />
          <button
            type="button"
            className="signin__button"
            onClick={() => void connect()}
            disabled={busy || organisation.trim() === ''}
          >
            {busy ? 'Opening GitHub…' : 'Create application'}
          </button>
        </>
      )}
    </div>
  )
}

/**
 * Hands the manifest to GitHub.
 *
 * A real form submission, because a manifest has to arrive as a POST and a
 * `fetch` would not navigate the browser to the approval screen.
 */
function postManifest(action: string, manifest: unknown): void {
  const form = document.createElement('form')
  form.method = 'POST'
  form.action = action

  const field = document.createElement('input')
  field.type = 'hidden'
  field.name = 'manifest'
  field.value = JSON.stringify(manifest)

  form.appendChild(field)
  document.body.appendChild(form)
  form.submit()
}
