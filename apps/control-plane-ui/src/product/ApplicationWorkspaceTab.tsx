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
import { ResourceEditor } from './ResourceEditor'

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
 *
 * The `switch` has no `default` for its nine real cases, and the one it
 * does have exists only to make a tenth tab a compile error rather than a
 * silent fallthrough: adding a case to {@link ApplicationTab}'s definition
 * without adding one here fails `exhaustive: never = tab` — assigning a
 * type that is not `never` to `never` — rather than quietly rendering
 * whatever the last case happens to be.
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
  switch (tab) {
    case 'Definition':
      return <ApplicationDefinitionTab appId={appId} draft={draft} onChange={onChange} />

    case 'Components':
      return (
        <ComponentEditor
          items={draft.components}
          onChange={(components) => {
            onChange({ ...draft, components })
          }}
        />
      )

    case 'Resources':
      return (
        <ResourceEditor
          items={draft.resources}
          onChange={(resources) => {
            onChange({ ...draft, resources })
          }}
        />
      )

    case 'Features':
      return (
        <FeatureEditor
          definition={draft}
          onChange={(features) => {
            onChange({ ...draft, features })
          }}
        />
      )

    case 'Plans':
      return (
        <PlanEditor
          definition={draft}
          onChange={(plans) => {
            onChange({ ...draft, plans })
          }}
        />
      )

    case 'Client configuration':
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

    case 'Navigation':
      return (
        <NavigationEditor
          definition={draft}
          onChange={(navigation) => {
            onChange({ ...draft, navigation })
          }}
        />
      )

    case 'Entry points':
      return <ApplicationEntryPointsTab draft={draft} onChange={onChange} />

    case 'Releases':
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

    default: {
      const exhaustive: never = tab
      return exhaustive
    }
  }
}
