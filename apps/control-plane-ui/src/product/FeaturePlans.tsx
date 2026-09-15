import { useEffect, useState } from 'react'
import type { ApplicationDefinition, ApplicationFeature, ApplicationPlan, Values } from '../api/catalogue-types'
import { ChoiceSet, Collection, Field } from './Forms'
export function FeatureEditor({ definition, onChange }: { definition: ApplicationDefinition; onChange: (features: ApplicationFeature[]) => void }) {
  return <Collection title="Features" items={definition.features} onChange={onChange} label={(item) => item.name} create={() => ({ id: '', name: '', description: '', implementedBy: [] })}>
    {(item, change) => <><Field label="Feature name" value={item.name} required onChange={(name) => { change({ ...item, name }) }} />
      <Field label="Feature ID" value={item.id} required onChange={(id) => { change({ ...item, id }) }} />
      <Field label="Description" value={item.description} onChange={(description) => { change({ ...item, description }) }} />
      <ChoiceSet label="Implemented by" choices={definition.components} value={item.implementedBy} onChange={(implementedBy) => { change({ ...item, implementedBy }) }} /></>}
  </Collection>
}
export function PlanEditor({ definition, onChange }: { definition: ApplicationDefinition; onChange: (plans: ApplicationPlan[]) => void }) {
  return <Collection title="Plans" items={definition.plans} onChange={onChange} label={(item) => item.name} create={() => ({ id: '', name: '', description: '', features: [], configuration: {} })}>
    {(item, change) => <><Field label="Plan name" value={item.name} required onChange={(name) => { change({ ...item, name }) }} />
      <Field label="Plan ID" value={item.id} required onChange={(id) => { change({ ...item, id }) }} />
      <Field label="Description" value={item.description} onChange={(description) => { change({ ...item, description }) }} />
      <ChoiceSet label="Features granted" choices={definition.features} value={item.features} onChange={(features) => { change({ ...item, features }) }} />
      <ValuesEditor value={item.configuration} onChange={(configuration) => { change({ ...item, configuration }) }} /></>}
  </Collection>
}
export function ValuesEditor({ value, onChange }: { value: Values; onChange: (value: Values) => void }) {
  const [text, setText] = useState(() => Object.entries(value).map(([key, entry]) => `${key}=${entry}`).join('\n'))
  useEffect(() => {
    if (JSON.stringify(parseLimits(text)) !== JSON.stringify(value)) setText(Object.entries(value).map(([key, entry]) => `${key}=${entry}`).join('\n'))
  }, [value, text])
  return <label className="form-field"><span>Plan limits (one key=value per line)</span><textarea rows={4} value={text} onChange={(event) => {
    setText(event.target.value)
    onChange(parseLimits(event.target.value))
  }} /></label>
}

function parseLimits(text: string): Values {
  return Object.fromEntries(text.split('\n').filter((line) => line.trim()).map((line): [string, string] => {
    const split = line.indexOf('=')
    return split < 0 ? [line.trim(), ''] : [line.slice(0, split).trim(), line.slice(split + 1)]
  }))
}
