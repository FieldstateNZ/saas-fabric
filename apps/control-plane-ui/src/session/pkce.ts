/**
 * Proof Key for Code Exchange, generated in the browser.
 *
 * The browser makes a random `verifier`, sends only its SHA-256 `challenge` to
 * the identity provider, and later presents the verifier when redeeming the
 * authorization code. Anyone who intercepts the code cannot redeem it without
 * the verifier, which never left this tab.
 *
 * The console is a public client: it holds no client secret, because a secret
 * shipped to a browser is not a secret. PKCE is what replaces one.
 */

/** A verifier and the challenge derived from it. */
export interface Pkce {
  readonly verifier: string
  readonly challenge: string
}

/**
 * A fresh verifier and its S256 challenge.
 *
 * 32 random bytes, which is comfortably inside the 43–128 character range RFC
 * 7636 requires once base64url-encoded, and well beyond guessing.
 */
export async function generate(): Promise<Pkce> {
  const verifier = randomToken(32)
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(verifier))

  return { verifier, challenge: base64url(new Uint8Array(digest)) }
}

/**
 * A random value for the `state` parameter.
 *
 * Its job is narrow and worth stating: the provider returns it verbatim, and
 * comparing it to what this tab stored is what makes a callback this tab did
 * not start recognisable as one to refuse.
 */
export function randomState(): string {
  return randomToken(16)
}

/** `bytes` cryptographically random bytes, base64url-encoded. */
function randomToken(bytes: number): string {
  return base64url(crypto.getRandomValues(new Uint8Array(bytes)))
}

/** Base64url without padding, as every OAuth parameter wants it. */
function base64url(bytes: Uint8Array): string {
  let binary = ''
  for (const byte of bytes) {
    binary += String.fromCharCode(byte)
  }

  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}
