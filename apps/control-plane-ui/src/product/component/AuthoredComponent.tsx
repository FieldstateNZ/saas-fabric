import { useState } from 'react'

import type { ApplicationComponent } from '../../api/component-types'
import { Field } from '../Field'
import { Select } from '../Select'
import { KindSelect } from './KindSelect'
import { blockedReason, isSaved, type ComponentSelecting } from './selecting'
import { VersionPicker } from './VersionPicker'

/** The closed list of platform capabilities a `capability` component can name (ADR 0021). */
const CAPABILITIES = [
  'Identity',
  'Database',
  'Secrets',
  'Authorization',
  'Routing',
  'Object storage',
  'Messaging',
].map((capability) => ({ value: capability, label: capability }))

/**
 * A `container`, `helm` or `capability` component, authored as free text as
 * it always was.
 *
 * A capability has no reference to build or deploy, so it names one of the
 * closed list instead. A saved container or chart offers "Select a
 * component artifact instead", which converts it in place through the same
 * selection a described component uses -- keeping its id, name, whether
 * every plan includes it, and its policy, so the features naming it are
 * kept (ADR 0026 section 7).
 */
export function AuthoredComponent({
  item,
  change,
  selecting,
}: {
  readonly item: ApplicationComponent
  readonly change: (item: ApplicationComponent) => void
  readonly selecting: ComponentSelecting
}) {
  const [converting, setConverting] = useState(false)
  const saved = isSaved(selecting, item.id)

  return (
    <>
      <Field label="Component name" required value={item.name} onChange={(name) => { change({ ...item, name }) }} />
      <Field label="Component ID" required value={item.id} onChange={(id) => { change({ ...item, id }) }} />
      <KindSelect item={item} choosable={!saved} onChange={change} />
      {item.kind === 'capability' ? (
        <Select
          label="Capability"
          value={item.reference}
          options={CAPABILITIES}
          onChange={(reference) => { change({ ...item, reference }) }}
        />
      ) : (
        <>
          <Field
            label={item.kind === 'helm' ? 'Chart reference' : 'Image reference'}
            required
            value={item.reference}
            onChange={(reference) => { change({ ...item, reference }) }}
          />
          <Field
            label="Version or digest"
            value={item.version}
            hint="Required before publishing."
            onChange={(version) => { change({ ...item, version }) }}
          />
        </>
      )}
      {saved && item.kind !== 'capability' && !converting && (
        <button type="button" onClick={() => { setConverting(true) }}>
          Select a component artifact instead
        </button>
      )}
      {converting && (
        <VersionPicker
          blocked={blockedReason(selecting, item.id)}
          select={(repository, version) => selecting.select(item.id, repository, version)}
          onSelected={() => { setConverting(false) }}
          onCancel={() => { setConverting(false) }}
          onReload={selecting.reload}
        />
      )}
    </>
  )
}
