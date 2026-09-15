import type { ApplicationRelease } from '../api/catalogue-types'
import { Panel } from '../console/Panel'
import { Field } from './Field'

/**
 * The Releases tab of `ApplicationWorkspace`: publish the current draft as
 * the next immutable version, and inspect every version published before it.
 *
 * Publishing is refused (`disabled`) while the draft has unsaved changes or
 * is already published, or the note is empty — a release without a note
 * tells a future operator nothing about what changed, and an unsaved draft
 * would publish state nobody has confirmed yet.
 */
export function ApplicationReleasesTab({
  note,
  onNoteChange,
  dirty,
  published,
  onPublish,
  releases,
}: {
  note: string
  onNoteChange: (note: string) => void
  dirty: boolean
  published: boolean
  onPublish: () => Promise<void>
  releases: readonly ApplicationRelease[]
}) {
  return (
    <>
      <Panel title="Publish definition">
        <div className="panel-body">
          <Field label="Release note" value={note} onChange={onNoteChange} />
          <div className="form-actions">
            <button
              type="button"
              className="primary-button"
              disabled={dirty || published || !note.trim()}
              onClick={() => void onPublish()}
            >
              Publish next version
            </button>
          </div>
          {dirty && <p>Save your draft before publishing.</p>}
        </div>
      </Panel>
      {[...releases].reverse().map((release) => (
        <Panel
          key={release.version}
          title={`Definition v${String(release.version)}`}
          action={
            <span className="mono">{new Date(release.publishedAt * 1000).toLocaleString()}</span>
          }
        >
          <div className="panel-body">
            <p>{release.note}</p>
            <p>
              {release.definition.components.length} components · {release.definition.plans.length}{' '}
              plans
            </p>
            <details>
              <summary>Inspect published definition</summary>
              <pre className="definition-preview">
                {JSON.stringify(release.definition, null, 2)}
              </pre>
            </details>
          </div>
        </Panel>
      ))}
    </>
  )
}
