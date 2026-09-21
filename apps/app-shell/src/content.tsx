import { useState } from 'react'
import { Button, Card, Input, Tag, Typography } from 'antd'
import type { ShellConfig } from './config/schema'

function Welcome() {
  return <Card>
    <Tag>Local prototype</Tag>
    <Typography.Title level={2}>A place to start.</Typography.Title>
    <Typography.Paragraph>
      This application’s branding, navigation and layout come from a configuration file.
      Its pages are registered React components.
    </Typography.Paragraph>
    <div className="grid gap-4 sm:grid-cols-2">
      <div className="rounded-lg border border-solid border-stone-200 p-4">
        <Typography.Text strong>One readable configuration</Typography.Text>
        <p className="mt-2 text-sm text-stone-600">Choose the name, color, page labels and regions in YAML.</p>
      </div>
      <div className="rounded-lg border border-solid border-stone-200 p-4">
        <Typography.Text strong>Small, registered components</Typography.Text>
        <p className="mt-2 text-sm text-stone-600">React supplies the content. The file chooses where it belongs.</p>
      </div>
    </div>
  </Card>
}

function Notes() {
  const [note, setNote] = useState('A little space for your application content.')
  return <Card title="Workspace note">
    <label htmlFor="workspace-note" className="mb-2 block">Try a React interaction</label>
    <Input.TextArea id="workspace-note" rows={4} value={note}
      onChange={(event) => { setNote(event.target.value) }} />
    <Button className="mt-4" type="primary" onClick={() => { setNote('') }} disabled={!note}>Clear note</Button>
    <p className="mt-3 text-sm text-stone-500">Sample content only. The note resets when you leave this page.</p>
  </Card>
}

export const pageRegistry = { welcome: Welcome, notes: Notes }

export function ConfigurationSummary({ config }: { config: ShellConfig }) {
  return <Card size="small" title="Configuration at a glance">
    <div className="grid gap-4 sm:grid-cols-3">
      <div><span className="text-sm text-stone-500">Application</span><p className="mt-1 font-medium">{config.branding.name}</p></div>
      <div><span className="text-sm text-stone-500">Pages</span><p className="mt-1 font-medium">{config.navigation.map((item) => item.label).join(' · ')}</p></div>
      <div><span className="text-sm text-stone-500">Primary color</span><p className="mt-1 flex items-center gap-2 font-mono text-sm">
        <span className="inline-block h-3 w-3 rounded-full" style={{ background: config.theme.primary }} />{config.theme.primary}
      </p></div>
    </div>
    <p className="mt-4 text-xs text-stone-500">Edit src/config/app.yaml to change this prototype.</p>
  </Card>
}
