/**
 * The tab bar shared by `ApplicationWorkspace` and `ClientWorkspace`.
 *
 * Both workspaces page their content the same way — a row of named sections,
 * one current at a time — and duplicating this markup per workspace was
 * exactly the kind of drift a shared component exists to prevent: a style or
 * accessibility fix here reaches both without needing to be made twice.
 */
export function TabNav<T extends string>({
  label,
  tabs,
  current,
  onChange,
}: {
  label: string
  tabs: readonly T[]
  current: T
  onChange: (tab: T) => void
}) {
  return (
    <nav className="tabs" aria-label={label}>
      {tabs.map((name) => (
        <button
          key={name}
          className={`tabs__tab${current === name ? ' tabs__tab--current' : ''}`}
          aria-current={current === name ? 'page' : undefined}
          onClick={() => {
            onChange(name)
          }}
        >
          {name}
        </button>
      ))}
    </nav>
  )
}
