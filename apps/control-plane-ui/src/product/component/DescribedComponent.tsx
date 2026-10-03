import { useState } from 'react'

import type { ApplicationComponent } from '../../api/component-types'
import { Field } from '../Field'
import { KindSelect } from './KindSelect'
import { ResolutionView } from './ResolutionView'
import { blockedReason, isSaved, type ComponentSelecting } from './selecting'
import { VersionPicker } from './VersionPicker'

/**
 * A described component: selected by its primary image's repository and a
 * version tag, and resolved by the server (ADR 0026 section 7).
 *
 * # Pending, then resolved
 *
 * One added here has no resolution: it is its id and a picker, because only
 * selecting a version creates it on the server -- named by its component
 * descriptor's title, in no plan by default, with a manual policy, each of
 * which the operator can change afterwards. Once resolved, its name is the
 * operator's and everything else is read-only, and "Choose another
 * version" re-opens the picker at the repository it records. Its id is
 * fixed then: a save naming a described component by another id names one
 * with no resolution, which the server refuses.
 */
export function DescribedComponent({
  item,
  change,
  selecting,
}: {
  readonly item: ApplicationComponent
  readonly change: (item: ApplicationComponent) => void
  readonly selecting: ComponentSelecting
}) {
  const [picking, setPicking] = useState(false)
  const pick = (repository: string, version: string) =>
    selecting.select(item.id, repository, version)
  const blocked = blockedReason(selecting, item.id)

  if (item.resolution === undefined) {
    return (
      <>
        <Field
          label="Component ID"
          required
          value={item.id}
          onChange={(id) => {
            change({ ...item, id })
          }}
        />
        <KindSelect item={item} choosable={!isSaved(selecting, item.id)} onChange={change} />
        <p className="support-note">
          Choose a component artifact. Once a version is selected, the component is named by its
          descriptor’s title, in no plan by default, with a manual update policy.
        </p>
        <VersionPicker blocked={blocked} select={pick} onSelected={() => undefined} onReload={selecting.reload} />
      </>
    )
  }

  return (
    <>
      <Field
        label="Component name"
        required
        value={item.name}
        onChange={(name) => {
          change({ ...item, name })
        }}
      />
      <Field label="Component ID" value={item.id} readOnly onChange={() => undefined} />
      <KindSelect item={item} choosable={false} onChange={change} />
      <ResolutionView resolution={item.resolution} />
      {picking ? (
        <VersionPicker
          initialRepository={item.resolution.repository}
          blocked={blocked}
          select={pick}
          onSelected={() => {
            setPicking(false)
          }}
          onCancel={() => {
            setPicking(false)
          }}
          onReload={selecting.reload}
        />
      ) : (
        <button
          type="button"
          onClick={() => {
            setPicking(true)
          }}
        >
          Choose another version
        </button>
      )}
    </>
  )
}
