import { useEffect, useState } from 'react'

import { getProduct } from '../api/catalogue'
import type { Catalogue, ClientProductResponse } from '../api/catalogue-types'
import { PageHeader } from '../console/PageHeader'
import { TabNav } from '../console/TabNav'
import { describe } from '../hooks/useClients'
import { CLIENT_TABS, type ClientTab } from './clientWorkspaceTabs'
import { ClientForm } from './ClientForm'
import { ClientShell } from './ClientShell'
import { ClientWorkspaceTab } from './ClientWorkspaceTab'
import { useConfigureClient } from './useConfigureClient'

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
 * edit on the Identity tab is real and writes its own activity entry into
 * the same client document, moving its revision, and nothing here observes
 * that when it happens. Opening `ClientForm` against a cached `data` would
 * seed it with a revision the server has already moved past, so the save
 * would be refused as a conflict against a change that did happen, just not
 * one this component knew about. `useConfigureClient`'s `open` re-reads the
 * product before switching into edit mode, so the form is always seeded
 * from what the server holds right now — see `ClientForm`'s own doc for the
 * other half of this: what happens when the revision moves again while the
 * form is open. The Configure button's own `disabled={configure.opening}`
 * stops a second read: `open` already refuses to start one while another is
 * in flight, but a second click getting through to a no-op is still a click
 * that looks like it did nothing. Preview and the tab bar are disabled for a
 * different reason — neither performs a read of its own (see `TabNav`'s own
 * doc), so there is no race to stop there. Disabling them is a precaution
 * against navigating to a view this component is about to replace out from
 * under the operator, the moment `onOpened` swaps it into `editing`.
 *
 * `staleNotice` has nothing to do with that read at all. It is set by
 * `noteStale`, called from `onStale` below (`:127-131`) when `ClientForm`'s
 * own reload-after-conflict — a save refused as stale, then re-read — finds
 * the client has moved again while the form was open. It exists so the
 * operator is told their edits there were not saved, rather than the
 * workspace just quietly showing newer data.
 *
 * This file sits in file-size-policy.md's 121-150 line band. It is one
 * state machine — loading, an error, editing, previewing, or the workspace
 * itself — and every branch needs the same `id`/`catalogue`/`onSaved` in
 * scope; splitting the branches apart would turn one component's states
 * into several components secretly sharing a lifecycle. The tab content and
 * the Configure-read behaviour are already their own files,
 * `ClientWorkspaceTab` and `useConfigureClient`.
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
  const [preview, setPreview] = useState(false)
  const [tab, setTab] = useState<ClientTab>('Overview')
  const [generation, setGeneration] = useState(0)

  const configure = useConfigureClient(id, {
    onOpened: (fresh) => {
      setData(fresh)
      setError(null)
      setEditing(true)
    },
    onFailed: setError,
  })

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
          configure.noteStale()
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
            disabled={configure.opening}
            onClick={() => {
              void configure.open()
            }}
          >
            {configure.opening ? 'Loading…' : 'Configure client'}
          </button>
        }
      />
      {configure.staleNotice && (
        <p role="status">
          This client changed while Configure was open, so it was re-read and your edits there
          were not saved.
        </p>
      )}
      <TabNav
        label="Client sections"
        tabs={CLIENT_TABS}
        current={tab}
        disabled={configure.opening}
        onChange={(next) => {
          setTab(next)
          configure.dismissStaleNotice()
        }}
      />
      <ClientWorkspaceTab
        tab={tab}
        data={data}
        catalogue={catalogue}
        previewDisabled={configure.opening}
        onPreview={() => {
          setPreview(true)
        }}
      />
    </>
  )
}
