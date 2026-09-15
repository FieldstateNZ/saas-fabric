import { useEffect, useState } from 'react'

import type { ProductApplication } from '../api/catalogue-types'
import { PageHeader } from '../console/PageHeader'
import { Status } from '../console/Status'
import { TabNav } from '../console/TabNav'
import { ApplicationWorkspaceTab } from './ApplicationWorkspaceTab'
import { SaveNotice } from './SaveNotice'
import type { CatalogueState } from './useCatalogue'

const tabs = [
  'Definition',
  'Components',
  'Features',
  'Client configuration',
  'Plans',
  'Navigation',
  'Entry points',
  'Releases',
] as const

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
 * earlier release keeps that exact version. `published` compares the draft
 * against the newest release by value, not by a flag the server sets, so the
 * badge reflects "nothing has changed since the last publish" rather than a
 * status that could drift from the definitions it describes.
 */
export function ApplicationWorkspace({
  app,
  state,
}: {
  app: ProductApplication
  state: CatalogueState
}) {
  const [draft, setDraft] = useState(app.draft)
  const [tab, setTab] = useState<(typeof tabs)[number]>('Definition')
  const [note, setNote] = useState('')
  const [success, setSuccess] = useState<string | null>(null)

  useEffect(() => {
    setDraft(app.draft)
  }, [app])

  const dirty = JSON.stringify(draft) !== JSON.stringify(app.draft)
  const published = JSON.stringify(app.releases.at(-1)?.definition) === JSON.stringify(app.draft)

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
          <Status value={published ? 'applied' : 'neutral'}>
            {published ? 'Published' : 'Draft changes'}
          </Status>
        }
      />
      <SaveNotice error={state.error} success={success} onReload={state.refresh} />
      <TabNav label="Application sections" tabs={tabs} current={tab} onChange={setTab} />
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
