import type { ApplicationDefinition, ApplicationRelease } from '../api/catalogue-types'
import { ApplicationDefinitionTab } from './ApplicationDefinitionTab'
import { ApplicationEntryPointsTab } from './ApplicationEntryPointsTab'
import { ApplicationReleasesTab } from './ApplicationReleasesTab'
import type { ApplicationTab } from './applicationWorkspaceTabs'
import { ComponentEditor } from './ComponentEditor'
import { FeatureEditor } from './FeatureEditor'
import { FieldDefinitions } from './FieldDefinitions'
import { NavigationEditor } from './NavigationEditor'
import { PlanEditor } from './PlanEditor'

/** What every tab of `ApplicationWorkspace` needs, whether or not it uses all of it. */
interface ApplicationWorkspaceTabProps {
  readonly appId: string
  readonly tab: ApplicationTab
  readonly draft: ApplicationDefinition
  readonly onChange: (draft: ApplicationDefinition) => void
  readonly note: string
  readonly onNoteChange: (note: string) => void
  readonly dirty: boolean
  readonly published: boolean
  readonly onPublish: () => Promise<void>
  readonly releases: readonly ApplicationRelease[]
}

/**
 * Chooses which section of an application's draft `ApplicationWorkspace`'s
 * current tab edits.
 *
 * Every editor here changes one slice of the same `draft` object — the
 * split exists so each slice has its own focused component, not because the
 * editors disagree about what they are editing.
 */
export function ApplicationWorkspaceTab({
  appId,
  tab,
  draft,
  onChange,
  note,
  onNoteChange,
  dirty,
  published,
  onPublish,
  releases,
}: ApplicationWorkspaceTabProps) {
  if (tab === 'Definition') {
    return <ApplicationDefinitionTab appId={appId} draft={draft} onChange={onChange} />
  }

  if (tab === 'Components') {
    return (
      <ComponentEditor
        items={draft.components}
        onChange={(components) => {
          onChange({ ...draft, components })
        }}
      />
    )
  }

  if (tab === 'Features') {
    return (
      <FeatureEditor
        definition={draft}
        onChange={(features) => {
          onChange({ ...draft, features })
        }}
      />
    )
  }

  if (tab === 'Plans') {
    return (
      <PlanEditor
        definition={draft}
        onChange={(plans) => {
          onChange({ ...draft, plans })
        }}
      />
    )
  }

  if (tab === 'Client configuration') {
    return (
      <>
        <p>Define non-secret values each client supplies. Credentials belong in Secrets.</p>
        <FieldDefinitions
          fields={draft.fields}
          onChange={(fields) => {
            onChange({ ...draft, fields })
          }}
        />
      </>
    )
  }

  if (tab === 'Navigation') {
    return (
      <NavigationEditor
        definition={draft}
        onChange={(navigation) => {
          onChange({ ...draft, navigation })
        }}
      />
    )
  }

  if (tab === 'Entry points') {
    return <ApplicationEntryPointsTab draft={draft} onChange={onChange} />
  }

  // `tab` is `ApplicationTab`, and every case above it has returned — this is
  // the last one, 'Releases', not a fallback for one that does not exist.
  return (
    <ApplicationReleasesTab
      note={note}
      onNoteChange={onNoteChange}
      dirty={dirty}
      published={published}
      onPublish={onPublish}
      releases={releases}
    />
  )
}
