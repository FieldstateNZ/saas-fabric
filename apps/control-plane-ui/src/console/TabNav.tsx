/**
 * The tab bar shared by `ApplicationWorkspace` and `ClientWorkspace`.
 *
 * Both workspaces page their content the same way — a row of named sections,
 * one current at a time — and duplicating this markup per workspace was
 * exactly the kind of drift a shared component exists to prevent: a style or
 * accessibility fix here reaches both without needing to be made twice.
 *
 * `disabled` exists for `ClientWorkspace`'s Configure: while a fresh read is
 * in flight for the form about to open, switching tabs underneath it would
 * read a client that is mid-navigation to somewhere else entirely.
 */
export function TabNav<T extends string>({
  label,
  tabs,
  current,
  onChange,
  disabled = false,
}: {
  label: string
  tabs: readonly T[]
  current: T
  onChange: (tab: T) => void
  disabled?: boolean
}) {
  return (
    <nav className="tabs" aria-label={label}>
      {tabs.map((name) => (
        <button
          key={name}
          className={`tabs__tab${current === name ? ' tabs__tab--current' : ''}`}
          aria-current={current === name ? 'page' : undefined}
          disabled={disabled}
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
