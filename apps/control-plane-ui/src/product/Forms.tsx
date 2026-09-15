import type { ReactNode } from 'react'
export function Field({ label, value, onChange, required = false, type = 'text', hint }: {
  label: string; value: string; onChange: (value: string) => void; required?: boolean; type?: string; hint?: string
}) {
  return <label className="form-field"><span>{label}{required && ' *'}</span><input type={type} value={value} required={required} maxLength={4096} onChange={(event) => { onChange(event.target.value) }} />{hint && <small>{hint}</small>}</label>
}
export function Select({ label, value, onChange, options }: { label: string; value: string; onChange: (value: string) => void; options: readonly { value: string; label: string }[] }) {
  return <label className="form-field"><span>{label}</span><select value={value} onChange={(event) => { onChange(event.target.value) }}>{options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select></label>
}
export function Check({ label, value, onChange }: { label: string; value: boolean; onChange: (value: boolean) => void }) {
  return <label className="form-check"><input type="checkbox" checked={value} onChange={(event) => { onChange(event.target.checked) }} />{label}</label>
}
export function ChoiceSet({ label, choices, value, onChange }: { label: string; choices: { id: string; name: string }[]; value: string[]; onChange: (value: string[]) => void }) {
  return <fieldset className="choice-set"><legend>{label}</legend>{choices.length === 0 && <p className="empty">No options defined yet.</p>}{choices.map((choice) => <Check key={choice.id} label={choice.name || choice.id} value={value.includes(choice.id)} onChange={(checked) => { onChange(checked ? [...value, choice.id] : value.filter((id) => id !== choice.id)) }} />)}</fieldset>
}
export function Collection<T>({ title, items, onChange, create, label, children }: { title: string; items: T[]; onChange: (items: T[]) => void; create: () => T; label: (item: T) => string; children: (item: T, change: (item: T) => void) => ReactNode }) {
  return <section className="collection"><div className="collection-heading"><h2>{title}</h2><button type="button" onClick={() => { onChange([...items, create()]) }}>+ Add {title.toLowerCase().replace(/s$/, '')}</button></div>
    {items.length === 0 && <p className="empty">No {title.toLowerCase()} yet.</p>}
    {items.map((item, index) => <details className="collection-item" key={index} open><summary>{label(item) || 'New item'}</summary><div className="form-grid">{children(item, (next) => { onChange(items.map((existing, i) => i === index ? next : existing)) })}</div>
      <button type="button" className="danger-link" onClick={() => { onChange(items.filter((_, i) => i !== index)) }}>Remove {label(item) || 'item'}</button></details>)}
  </section>
}
export function SaveNotice({ error, success, onReload }: { error: string | null; success: string | null; onReload?: () => void }) {
  return <>{error && <div className="error" role="alert">{error}{onReload && <p><button type="button" onClick={onReload}>Reload latest version</button></p>}</div>}{success && <p className="success-notice" role="status">{success}</p>}</>
}
