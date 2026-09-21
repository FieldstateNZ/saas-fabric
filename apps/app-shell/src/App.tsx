import { StyleProvider } from '@ant-design/cssinjs'
import { Alert, ConfigProvider } from 'antd'
import source from './config/app.yaml?raw'
import { parseConfig } from './config/parse'
import { ConfiguredShell } from './ConfiguredShell'

export function App({ yaml = source }: { yaml?: string }) {
  const result = parseConfig(yaml)
  return <StyleProvider layer><ConfigProvider theme={result.ok ? { token: {
    colorPrimary: result.config.theme.primary,
    borderRadius: result.config.theme.borderRadius,
    colorBgLayout: result.config.theme.background,
  } } : {}}>
    {result.ok ? <ConfiguredShell config={result.config} /> : <main className="mx-auto max-w-3xl p-6 sm:p-12">
      <Alert type="error" showIcon title="Check your application configuration" description={<>
        <p className="mb-3">Fix src/config/app.yaml and save the file.</p>
        <ul className="list-disc space-y-2 pl-5">{result.errors.map((error, index) =>
          <li className="whitespace-pre-wrap break-words" key={index}>{error}</li>)}</ul>
      </>} />
    </main>}
  </ConfigProvider></StyleProvider>
}
