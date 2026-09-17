import { SignIn } from './components/SignIn'
import { Brand } from './console/Brand'
import { Console } from './console/Console'
import { useSession } from './hooks/useSession'

/** Authentication finishes before the console makes any operator requests. */
export function App() {
  const session = useSession()

  if (session.state.status === 'checking') {
    return (
      <div className="session-loading">
        <Brand />
        <p role="status">Connecting to your platform…</p>
      </div>
    )
  }

  if (session.state.status === 'signed-out') {
    return <SignIn error={session.state.error} onSignIn={session.signIn} />
  }

  return <Console />
}
