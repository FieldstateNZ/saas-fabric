import { useEffect, useState } from 'react'

import { getProduct } from '../api/catalogue'
import type { Catalogue, ClientProductResponse } from '../api/catalogue-types'
import { IdentityPanel } from '../components/IdentityPanel'
import { Secrets } from '../components/tabs/Secrets'
import { PageHeader } from '../console/PageHeader'
import { TabNav } from '../console/TabNav'
import { describe } from '../hooks/useClients'
import { ActivityTable } from './ActivityTable'
import { ClientApplicationsTab } from './ClientApplicationsTab'
import { ClientConfigurationTab } from './ClientConfigurationTab'
import { ClientDomainsTab } from './ClientDomainsTab'
import { ClientForm } from './ClientForm'
import { ClientHealthTab } from './ClientHealthTab'
import { ClientOverviewTab } from './ClientOverviewTab'
import { ClientShell } from './ClientShell'

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
  }, [id, generation, tab])

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
            onClick={() => {
              setEditing(true)
            }}
          >
            Configure client
          </button>
        }
      />
      <TabNav label="Client sections" tabs={tabs} current={tab} onChange={setTab} />
      {tab === 'Overview' && (
        <ClientOverviewTab
          data={data}
          onPreview={() => {
            setPreview(true)
          }}
        />
      )}
      {tab === 'Applications' && <ClientApplicationsTab data={data} />}
      {tab === 'Configuration' && <ClientConfigurationTab data={data} catalogue={catalogue} />}
      {tab === 'Identity' && <IdentityPanel client={data.client} />}
      {tab === 'Secrets' && <Secrets client={data.client} />}
      {tab === 'Domains' && <ClientDomainsTab data={data} />}
      {tab === 'Activity' && <ActivityTable activity={data.product.activity} />}
      {tab === 'Health' && <ClientHealthTab data={data} />}
    </>
  )
}
