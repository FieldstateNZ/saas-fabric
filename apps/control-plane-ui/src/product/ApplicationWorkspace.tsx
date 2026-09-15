import { useEffect, useState } from 'react'

import type { ProductApplication } from '../api/catalogue-types'
import { PageHeader } from '../console/PageHeader'
import { Status } from '../console/Status'
import { TabNav } from '../console/TabNav'
import { APPLICATION_TABS, type ApplicationTab } from './applicationWorkspaceTabs'
import { ApplicationWorkspaceTab } from './ApplicationWorkspaceTab'
import { SaveNotice } from './SaveNotice'
import type { CatalogueState } from './useCatalogue'

/**
 * Editing one application: its draft definition, and every version ever
 * published from it.
 *
 * # A save is a draft; a publish is a release
 *
 * `save` writes the draft as-is — an operator can save an unfinished
 * definition and come back to it. `publish` is the act that makes it
 * immutable: it copies the current draft into a new numbered
 * {@link ApplicationRelease}, and every client already assigned to an
 * earlier release keeps that exact version.
 *
 * `published` compares the definition currently being edited — including
 * unsaved local changes — against the newest release, by value rather than a
 * flag the server sets. It is checked against `draft`, not `app.draft`
 * (the last value actually saved): the header must stop claiming "Published"
 * the moment a local edit diverges from that release, even before the
 * operator has saved it.
 */
export function ApplicationWorkspace({
  app,
  state,
}: {
  app: ProductApplication
  state: CatalogueState
}) {
  const [draft, setDraft] = useState(app.draft)
  const [tab, setTab] = useState<ApplicationTab>('Definition')
  const [note, setNote] = useState('')
  const [success, setSuccess] = useState<string | null>(null)

  useEffect(() => {
    setDraft(app.draft)
  }, [app])

  const dirty = JSON.stringify(draft) !== JSON.stringify(app.draft)
  const published = JSON.stringify(app.releases.at(-1)?.definition) === JSON.stringify(draft)

  async function save() {
    if (await state.save({ action: 'saveApplication', id: app.id, definition: draft })) {
      setSuccess('Draft saved. Publish it when it is ready for client assignments.')
    }
  }

  async function publish() {
    if (await state.save({ action: 'publishApplication', id: app.id, note })) {
      setNote('')
      setSuccess('Definition published. Existing clients retain their assigned version.')
    }
  }

  return (
    <>
      <a href="#/applications" className="back-link">
        ← All applications
      </a>
      <PageHeader
        eyebrow="Application"
        title={app.draft.name}
        actions={
          <Status value={published ? 'published' : 'neutral'}>
            {published ? 'Published' : 'Draft changes'}
          </Status>
        }
      />
      <SaveNotice
        error={state.saveError}
        success={success}
        onReload={state.conflict ? state.refresh : undefined}
      />
      <TabNav label="Application sections" tabs={APPLICATION_TABS} current={tab} onChange={setTab} />
      <form
        onSubmit={(event) => {
          event.preventDefault()
          void save()
        }}
      >
        <fieldset disabled={state.saving}>
          <ApplicationWorkspaceTab
            appId={app.id}
            tab={tab}
            draft={draft}
            onChange={setDraft}
            note={note}
            onNoteChange={setNote}
            dirty={dirty}
            published={published}
            onPublish={publish}
            releases={app.releases}
          />
          <div className="form-actions">
            <button className="primary-button" type="submit" disabled={!dirty}>
              {state.saving ? 'Saving…' : 'Save draft'}
            </button>
            <button
              type="button"
              disabled={!dirty}
              onClick={() => {
                setDraft(app.draft)
              }}
            >
              Discard changes
            </button>
            {dirty && <span>Unsaved changes</span>}
          </div>
        </fieldset>
      </form>
    </>
  )
}
