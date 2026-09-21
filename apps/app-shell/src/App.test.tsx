import { fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { App } from './App'
import example from './config/app.yaml?raw'

beforeEach(() => { window.location.hash = '' })

it('renders YAML branding, region order and registered content', () => {
  const yaml = example.replace('name: SaaS Fabric', 'name: Example Studio')
    .replace('content: [page, configuration]', 'content: [configuration, page]')
    .replace('footer: [footer]', 'footer: []')
  render(<App yaml={yaml} />)
  expect(screen.getAllByText('Example Studio')).toHaveLength(2)
  expect(screen.getByRole('heading', { name: 'Overview', level: 1 })).toBeVisible()
  expect(screen.queryByText('SaaS Fabric · Local prototype')).not.toBeInTheDocument()
  const summary = screen.getByText('Configuration at a glance')
  expect(summary.compareDocumentPosition(screen.getByText('A place to start.')) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
})

it('allows navigation in the header and omits an empty sidebar', () => {
  render(<App yaml={example.replace('header: [brand]', 'header: [brand, navigation]').replace('regions: [navigation]', 'regions: []')} />)
  expect(within(screen.getByRole('banner')).getByRole('navigation')).toBeVisible()
  expect(document.getElementById('shell-sidebar')).toBeNull()
})

it('loads the configured page on hash navigation and supports its React interaction', async () => {
  const user = userEvent.setup()
  render(<App yaml={example} />)
  window.location.hash = '#/workspace'
  fireEvent(window, new HashChangeEvent('hashchange'))
  expect(screen.getByRole('heading', { name: 'Workspace', level: 1 })).toBeVisible()
  expect(screen.getByRole('link', { name: 'Workspace' })).toHaveAttribute('aria-current', 'page')
  await user.click(screen.getByRole('button', { name: 'Clear note' }))
  expect(screen.getByRole('textbox')).toHaveValue('')
})

it('honors collapse configuration and shows errors without rendering a partial shell', () => {
  const { rerender } = render(<App yaml={example.replace('collapsible: true', 'collapsible: false')} />)
  expect(screen.queryByRole('button', { name: 'Close menu' })).not.toBeInTheDocument()
  rerender(<App yaml={example.replace("primary: '#4a5c42'", 'primary: invalid')} />)
  expect(screen.getByText('Check your application configuration')).toBeVisible()
  expect(screen.getByText(/theme.primary:/)).toBeVisible()
  expect(screen.queryByRole('navigation')).not.toBeInTheDocument()
})
