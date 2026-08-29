import { useState } from 'react'

import { beginConnection, beginInstall } from '../api/client'
import type { Integration } from '../api/types'
import { describe } from '../hooks/useClients'

/**
 * Connecting this platform to where client configuration is kept.
 *
 * # Two approvals, so two buttons
 *
 * Creating the application on the Git host and installing it are separate
 * approvals there, and an operator can complete the first and abandon the
 * second. The console shows whichever step is outstanding rather than
 * pretending it is one action that sometimes half-works.
 */
interface GitIntegrationProps {
  readonly integration: Integration
}

export function GitIntegration({ integration }: GitIntegrationProps) {
  const [organisation, setOrganisation] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // A deployment that states its repository has opted out. Offering a control
  // that would overwrite that from a browser would be offering to undo it.
  if (!integration.managed) {
    return null
  }

  async function connect(): Promise<void> {
    setBusy(true)
    setError(null)

    try {
      const { post_url, manifest } = await beginConnection(organisation.trim())
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
      const { url } = await beginInstall()
      window.location.assign(url)
    } catch (thrown: unknown) {
      setError(describe(thrown))
      setBusy(false)
    }
  }

  const created = integration.application !== null

  return (
    <div className="connect">
      {error !== null && <p className="error">{error}</p>}

      {created ? (
        <>
          <p className="connect__lead">
            SaaS Fabric has an application on GitHub. Install it on the organisation whose client
            configuration it should read.
          </p>
          <button type="button" className="signin__button" onClick={() => void install()} disabled={busy}>
            {busy ? 'Opening GitHub…' : 'Install on GitHub'}
          </button>
        </>
      ) : (
        <>
          <p className="connect__lead">
            SaaS Fabric will create its own GitHub application in your organisation. You approve it
            on GitHub &mdash; there is no application to make by hand and no key to copy.
          </p>
          <label className="connect__label" htmlFor="organisation">
            GitHub organisation
          </label>
          <input
            id="organisation"
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
