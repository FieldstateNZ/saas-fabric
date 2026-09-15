import type { AssignmentRequest, ProductApplication } from '../api/catalogue-types'
import { Check, Select } from './Forms'
import { ConfigurationInputs } from './ConfigurationInputs'
export function Assignments({ apps, value, onChange, locked = [] }: { apps: ProductApplication[]; value: AssignmentRequest[]; onChange: (value: AssignmentRequest[]) => void; locked?: string[] }) {
  const available = apps.filter((app) => app.releases.length > 0)
  const change = (next: AssignmentRequest) => { onChange(value.map((item) => item.applicationId === next.applicationId ? next : item)) }
  return <div className="assignment-list">{available.length === 0 && <p>No published applications yet. Publish an application with at least one plan to assign it.</p>}{available.map((app) => {
    const assignment = value.find((item) => item.applicationId === app.id)
    const latest = app.releases.at(-1)
    const release = app.releases.find((r) => r.version === assignment?.version)
    return <section className="assignment" key={app.id}>
      <Check label={app.draft.name} value={Boolean(assignment)} onChange={(checked) => {
        if (checked && latest?.definition.plans[0]) onChange([...value, { applicationId: app.id, version: latest.version, planId: latest.definition.plans[0].id, configuration: {} }])
        else if (!locked.includes(app.id)) onChange(value.filter((item) => item.applicationId !== app.id))
      }} />
      {assignment && release && <><div className="form-grid"><Select label={`${app.draft.name} version`} value={String(assignment.version)} options={app.releases.map((r) => ({ value: String(r.version), label: `Definition v${String(r.version)}` }))} onChange={(version) => {
        const selected = app.releases.find((r) => r.version === Number(version)); const plan = selected?.definition.plans[0]
        if (selected && plan) change({ ...assignment, version: selected.version, planId: plan.id, configuration: {} })
      }} /><Select label={`${app.draft.name} plan`} value={assignment.planId} options={release.definition.plans.map((p) => ({ value: p.id, label: p.name }))} onChange={(planId) => { change({ ...assignment, planId }) }} /></div>
        <ConfigurationInputs fields={release.definition.fields} values={assignment.configuration} onChange={(configuration) => { change({ ...assignment, configuration }) }} />
        <p className="support-note">Features: {release.definition.plans.find((plan) => plan.id === assignment.planId)?.features.join(', ') || 'Core components only'}</p>
        {locked.includes(app.id) && <p className="support-note">This application is assigned. Removal requires deprovisioning.</p>}</>}
    </section>
  })}</div>
}
