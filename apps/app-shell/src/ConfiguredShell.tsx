import { useEffect, useState } from 'react'
import { Button, Layout, Menu, Typography } from 'antd'
import type { ShellConfig } from './config/schema'
import { ConfigurationSummary, pageRegistry } from './content'

function routeId() { return window.location.hash.replace(/^#\//, '') }

export function ConfiguredShell({ config }: { config: ShellConfig }) {
  const [route, setRoute] = useState(routeId)
  const [collapsed, setCollapsed] = useState(false)
  const [narrow, setNarrow] = useState(false)
  useEffect(() => {
    const update = () => { setRoute(routeId()) }
    window.addEventListener('hashchange', update)
    return () => { window.removeEventListener('hashchange', update) }
  }, [])
  const page = config.navigation.find((item) => item.id === route) ?? config.navigation[0]
  useEffect(() => { document.title = `${page?.label ?? ''} · ${config.branding.name}` }, [page?.label, config.branding.name])
  if (!page) return null // Validation guarantees at least one navigation entry.
  const activeId = page.id
  const Page = pageRegistry[page.content]
  const hasSidebar = config.layout.sidebar.regions.length > 0
  const canCollapse = config.layout.sidebar.collapsible
  function navigation(horizontal: boolean) {
    return <nav aria-label="Application navigation" className="min-w-0 flex-1">
      <Menu mode={horizontal ? 'horizontal' : 'inline'} selectedKeys={[activeId]}
        className="border-none" items={config.navigation.map((item) => ({ key: item.id,
          label: <a href={`#/${item.id}`} aria-current={activeId === item.id ? 'page' : undefined}
            onClick={() => { if (narrow && canCollapse) setCollapsed(true) }}>{item.label}</a>,
        }))} />
    </nav>
  }
  function brand() {
    return <div className="flex min-w-0 items-center gap-3">
      <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg text-lg font-semibold text-white"
        style={{ background: config.theme.primary }}>{config.branding.mark}</span>
      <div className="min-w-0"><div className="truncate text-base font-semibold">{config.branding.name}</div>
        <div className="truncate text-xs text-stone-500">{config.branding.subtitle}</div></div>
    </div>
  }
  return <Layout className="min-h-dvh" style={{ background: config.theme.background }}>
    <a className="skip-link" href="#main-content" onClick={(event) => {
      event.preventDefault(); document.getElementById('main-content')?.focus()
    }}>Skip to content</a>
    {(config.layout.header.length > 0 || (hasSidebar && canCollapse)) &&
      <Layout.Header className="flex h-auto min-h-18 flex-wrap items-center gap-4 border-b border-solid border-stone-200 bg-white px-4 py-4 leading-normal sm:px-6">
        {hasSidebar && canCollapse && <Button aria-controls="shell-sidebar" aria-expanded={!collapsed}
          onClick={() => { setCollapsed(!collapsed) }}>{collapsed ? 'Open menu' : 'Close menu'}</Button>}
        {config.layout.header.map((id) => <div className={id === 'navigation' ? 'min-w-0 flex-1' : 'min-w-0'} key={id}>
          {id === 'brand' ? brand() : navigation(!narrow)}</div>)}
      </Layout.Header>}
    <Layout className="min-w-0 flex-1 bg-transparent">
      {hasSidebar && <Layout.Sider id="shell-sidebar" theme="light" width={config.layout.sidebar.width}
        collapsedWidth={0} collapsed={canCollapse && collapsed} trigger={null} breakpoint="md"
        onBreakpoint={(broken) => { setNarrow(broken); if (canCollapse) setCollapsed(broken) }}
        className={narrow ? 'mobile-sidebar border-b border-solid border-stone-200' : 'border-r border-solid border-stone-200'}>
        {(!canCollapse || !collapsed) && <div className="space-y-5 p-4">
          {config.layout.sidebar.regions.map((id) => <div key={id}>{id === 'brand' ? brand() : navigation(false)}</div>)}
        </div>}
      </Layout.Sider>}
      <Layout className="min-w-0 w-full bg-transparent">
        <Layout.Content id="main-content" tabIndex={-1} className="mx-auto w-full max-w-6xl p-4 sm:p-8">
          <Typography.Title level={1} className="mb-6 text-3xl">{page.label}</Typography.Title>
          <div className="space-y-6">{config.layout.content.map((id) => id === 'page'
            ? <Page key={page.id} /> : <ConfigurationSummary key={id} config={config} />)}</div>
        </Layout.Content>
        {config.layout.footer.length > 0 && <Layout.Footer className="bg-transparent px-4 py-5 text-center text-xs text-stone-500">
          {config.branding.footer}
        </Layout.Footer>}
      </Layout>
    </Layout>
  </Layout>
}
