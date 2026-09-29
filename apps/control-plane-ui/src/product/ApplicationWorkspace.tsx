import { useEffect, useState } from 'react'

import type { ProductApplication } from '../api/catalogue-types'
import { PageHeader } from '../console/PageHeader'
import { Status } from '../console/Status'
import { TabNav } from '../console/TabNav'
import { APPLICATION_TABS, type ApplicationTab } from './applicationWorkspaceTabs'
import { ApplicationWorkspaceTab } from './ApplicationWorkspaceTab'
import type { ComponentSelecting } from './ComponentEditor'
import { keepingPending, sameDefinition, toDraftRequest, unsavedBesidesPending } from './draft-request'
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
 * # The local draft keeps what the server resolved
 *
 * `draft` holds each described component's resolution exactly as the
 * server stored it, so `dirty` and `published` compare like with like; a
 * save strips it only when it sends (`toDraftRequest`). Selecting a version
 * writes the saved draft, so the picker is disabled while anything besides
 * a component still waiting for its version is unsaved, and the catalogue
 * it answers with replaces this draft through `app`.
 *
 * A component waiting for its version is never sent -- the server can hold
 * none -- so it is not an unsaved change either: `dirty` leaves it out, and
 * a new `app` keeps it (`keepingPending`), so saving the other edits frees
 * the picker without losing it.
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
    setDraft((local) => keepingPending(app.draft, local))
  }, [app])

  const dirty = unsavedBesidesPending(draft, app.draft)
  const published = sameDefinition(app.releases.at(-1)?.definition, draft)
  const selecting: ComponentSelecting = {
    saved: app.draft.components,
    local: draft.components,
    unsaved: dirty,
    select: (component, repository, version) =>
      state.select({ action: 'selectComponentVersion', id: app.id, component, repository, version }),
    reload: state.refresh,
  }

  async function save() {
    if (await state.save({ action: 'saveApplication', id: app.id, definition: toDraftRequest(draft) })) {
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
            selecting={selecting}
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
