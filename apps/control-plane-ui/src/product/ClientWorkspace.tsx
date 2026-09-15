import { useEffect, useState } from 'react'

import { getProduct } from '../api/catalogue'
import type { Catalogue, ClientProductResponse } from '../api/catalogue-types'
import { PageHeader } from '../console/PageHeader'
import { TabNav } from '../console/TabNav'
import { describe } from '../hooks/useClients'
import { ClientForm } from './ClientForm'
import { ClientShell } from './ClientShell'
import { ClientWorkspaceTab } from './ClientWorkspaceTab'

const tabs = [
  'Overview',
  'Applications',
  'Configuration',
  'Identity',
  'Domains',
  'Activity',
  'Secrets',
  'Health',
] as const

/**
 * One client's full workspace: its resolved product state, and every way an
 * operator inspects or changes it.
 *
 * `editing` and `preview` replace the whole workspace rather than opening a
 * dialog over it — configuring a client (`ClientForm`) and previewing its
 * shell (`ClientShell`) are both full views in their own right, and layering
 * them as modals would mean nesting one page's navigation inside another's.
 *
 * # Configure always opens on a fresh read
 *
 * `data` can go stale without this component ever changing it: an identity
 * edit on the Identity tab writes its own activity entry into the same
 * client document, moving its revision, and nothing here observes that.
 * Opening `ClientForm` against a cached `data` would seed it with a
 * revision the server has already moved past, and every save would be
 * refused as a conflict against an edit that never happened. `openConfigure`
 * re-reads the product before switching into edit mode, so the form is
 * always seeded from what the server holds right now — see `ClientForm`'s
 * own doc for the other half of this: what happens when the revision moves
 * again while the form is open.
 *
 * This file sits in file-size-policy.md's 121-150 line band. It is one
 * state machine — loading, an error, editing, previewing, or the workspace
 * itself — and every branch needs the same `id`/`catalogue`/`onSaved` in
 * scope; splitting the branches apart would turn one component's states
 * into several components secretly sharing a lifecycle. The tab content
 * itself is already its own file, `ClientWorkspaceTab`.
 */
export function ClientWorkspace({
  id,
  catalogue,
  onSaved,
}: {
  id: string
  catalogue: Catalogue
  onSaved: () => void
}) {
  const [data, setData] = useState<ClientProductResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [editing, setEditing] = useState(false)
  const [opening, setOpening] = useState(false)
  const [preview, setPreview] = useState(false)
  const [tab, setTab] = useState<(typeof tabs)[number]>('Overview')
  const [generation, setGeneration] = useState(0)

  useEffect(() => {
    let active = true

    void getProduct(id).then(
      (value) => {
        if (active) {
          setData(value)
          setError(null)
        }
      },
      (error: unknown) => {
        if (active) {
          setError(describe(error))
        }
      },
    )

    return () => {
      active = false
    }
  }, [id, generation])

  async function openConfigure() {
    if (opening) {
      return
    }

    setOpening(true)
    try {
      setData(await getProduct(id))
      setError(null)
      setEditing(true)
    } catch (thrown: unknown) {
      setError(describe(thrown))
    } finally {
      setOpening(false)
    }
  }

  if (error) {
    return (
      <div className="error" role="alert">
        {error}
        <button
          onClick={() => {
            setGeneration((n) => n + 1)
          }}
        >
          Retry
        </button>
      </div>
    )
  }

  if (!data) {
    return <p role="status">Loading client…</p>
  }

  if (editing) {
    return (
      <ClientForm
        catalogue={catalogue}
        existing={data}
        onSaved={() => {
          setEditing(false)
          setGeneration((n) => n + 1)
          onSaved()
        }}
        onStale={(fresh) => {
          setData(fresh)
          setEditing(false)
        }}
        onCancel={() => {
          setEditing(false)
        }}
      />
    )
  }

  if (preview) {
    return (
      <ClientShell
        data={data}
        onBack={() => {
          setPreview(false)
        }}
      />
    )
  }

  return (
    <>
      <a className="back-link" href="#/clients">
        ← All clients
      </a>
      <PageHeader
        eyebrow="Client"
        title={data.client.displayName}
        actions={
          <button
            className="primary-button"
            disabled={opening}
            onClick={() => {
              void openConfigure()
            }}
          >
            {opening ? 'Loading…' : 'Configure client'}
          </button>
        }
      />
      <TabNav label="Client sections" tabs={tabs} current={tab} onChange={setTab} />
      <ClientWorkspaceTab
        tab={tab}
        data={data}
        catalogue={catalogue}
        onPreview={() => {
          setPreview(true)
        }}
      />
    </>
  )
}
