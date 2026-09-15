import { useEffect, useState } from 'react'
import type { ProductApplication } from '../api/catalogue-types'
import { PageHeader, Panel, Status } from '../console/primitives'
import { ComponentEditor } from './ComponentEditor'
import { FeatureEditor, PlanEditor } from './FeaturePlans'
import { FieldDefinitions } from './FieldDefinitions'
import { Field, SaveNotice } from './Forms'
import { NavigationEditor } from './NavigationEditor'
import type { CatalogueState } from './useCatalogue'
const tabs = ['Definition', 'Components', 'Features', 'Client configuration', 'Plans', 'Navigation', 'Entry points', 'Releases'] as const
export function ApplicationWorkspace({ app, state }: { app: ProductApplication; state: CatalogueState }) {
  const [draft, setDraft] = useState(app.draft)
  const [tab, setTab] = useState<(typeof tabs)[number]>('Definition')
  const [note, setNote] = useState('')
  const [success, setSuccess] = useState<string | null>(null)
  useEffect(() => { setDraft(app.draft) }, [app])
  const dirty = JSON.stringify(draft) !== JSON.stringify(app.draft)
  const published = JSON.stringify(app.releases.at(-1)?.definition) === JSON.stringify(app.draft)
  async function save() { if (await state.save({ action: 'saveApplication', id: app.id, definition: draft })) setSuccess('Draft saved. Publish it when it is ready for client assignments.') }
  async function publish() { if (await state.save({ action: 'publishApplication', id: app.id, note })) { setNote(''); setSuccess('Definition published. Existing clients retain their assigned version.') } }
  return <><a href="#/applications" className="back-link">← All applications</a><PageHeader eyebrow="Application" title={app.draft.name} actions={<Status value={published ? 'applied' : 'neutral'}>{published ? 'Published' : 'Draft changes'}</Status>} />
    <SaveNotice error={state.error} success={success} onReload={state.refresh} />
    <nav className="tabs" aria-label="Application sections">{tabs.map((name) => <button key={name} className={`tabs__tab${name === tab ? ' tabs__tab--current' : ''}`} aria-current={tab === name ? 'page' : undefined} onClick={() => { setTab(name) }}>{name}</button>)}</nav>
    <form onSubmit={(event) => { event.preventDefault(); void save() }}><fieldset disabled={state.saving}>
      {tab === 'Definition' && <Panel title="Application definition"><div className="panel-body form-grid"><Field label="Name" value={draft.name} required onChange={(name) => { setDraft({ ...draft, name }) }} />
        <Field label="Description" value={draft.description} onChange={(description) => { setDraft({ ...draft, description }) }} /><p className="support-note">Application ID: {app.id}. Published versions contain the complete component, feature, plan, field and navigation definitions.</p></div></Panel>}
      {tab === 'Components' && <ComponentEditor items={draft.components} onChange={(components) => { setDraft({ ...draft, components }) }} />}
      {tab === 'Features' && <FeatureEditor definition={draft} onChange={(features) => { setDraft({ ...draft, features }) }} />}
      {tab === 'Plans' && <PlanEditor definition={draft} onChange={(plans) => { setDraft({ ...draft, plans }) }} />}
      {tab === 'Client configuration' && <><p>Define non-secret values each client supplies. Credentials belong in Secrets.</p><FieldDefinitions fields={draft.fields} onChange={(fields) => { setDraft({ ...draft, fields }) }} /></>}
      {tab === 'Navigation' && <NavigationEditor definition={draft} onChange={(navigation) => { setDraft({ ...draft, navigation }) }} />}
      {tab === 'Entry points' && <Panel title="Client entry points"><div className="panel-body"><Field label="Application hostname template" value={draft.domain} onChange={(domain) => { setDraft({ ...draft, domain }) }} hint="For example {client}.workspec.io. Leave empty to use the client’s primary host." />
        <p>Each assignment registers public sign-in callbacks using PKCE S256. Hostname declarations do not prove routing or certificate readiness.</p></div></Panel>}
      {tab === 'Releases' ? <><Panel title="Publish definition"><div className="panel-body"><Field label="Release note" value={note} onChange={setNote} />
        <div className="form-actions"><button type="button" className="primary-button" disabled={dirty || published || !note.trim()} onClick={() => void publish()}>Publish next version</button></div>
        {dirty && <p>Save your draft before publishing.</p>}</div></Panel>
        {[...app.releases].reverse().map((release) => <Panel key={release.version} title={`Definition v${String(release.version)}`} action={<span className="mono">{new Date(release.publishedAt * 1000).toLocaleString()}</span>}>
          <div className="panel-body"><p>{release.note}</p><p>{release.definition.components.length} components · {release.definition.plans.length} plans</p><details><summary>Inspect published definition</summary><pre className="definition-preview">{JSON.stringify(release.definition, null, 2)}</pre></details></div></Panel>)}</>
        : null}
      <div className="form-actions"><button className="primary-button" type="submit" disabled={!dirty}>{state.saving ? 'Saving…' : 'Save draft'}</button><button type="button" disabled={!dirty} onClick={() => { setDraft(app.draft) }}>Discard changes</button>{dirty && <span>Unsaved changes</span>}</div>
    </fieldset></form>
  </>
}
