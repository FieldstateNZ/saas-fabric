"""Verify fixture brands through a running gateway; no Aspire dependency."""
import html
import json
from pathlib import Path
import re
import sys
import urllib.error
import urllib.parse
import urllib.request

base = sys.argv[1].rstrip('/')
fixtures = Path(__file__).resolve().parent
query = urllib.parse.urlencode(dict(
    client_id='app-shell', redirect_uri='http://127.0.0.1:5186/callback',
    response_type='code', scope='openid', code_challenge='A' * 43,
    code_challenge_method='S256'))


def get(url):
    with urllib.request.urlopen(url, timeout=15) as response:
        assert response.status == 200
        return response.read()


for client, realm in [('demo', 'fabric-demo'), ('contrast', 'fabric-contrast')]:
    expected = json.loads((fixtures / client / 'brand.json').read_text())
    discovery = json.loads(get(f'{base}/realms/{realm}/.well-known/openid-configuration'))
    assert discovery['issuer'] == f'{base}/realms/{realm}'
    assert json.loads(get(discovery['jwks_uri']))['keys']
    page = get(f'{base}/realms/{realm}/protocol/openid-connect/auth?{query}').decode()
    match = re.search(r'href="([^"]+/css/brand.css[^"]*)"', page)
    assert match, f'{client}: branded login stylesheet missing'
    css_url = urllib.parse.urljoin(base, html.unescape(match[1]))
    stylesheet = get(css_url).decode()
    assert all(expected[key] in stylesheet for key in ['primaryColor', 'backgroundColor', 'foregroundColor'])
    assert get(urllib.parse.urljoin(css_url, '../img/logo.png')) == (fixtures / client / 'logo.png').read_bytes()
    theme = re.search(r'/login/(fabric-[^/]+)/', css_url)[1]
    manifest = json.loads(get(f'{base}/brands/{theme}/brand.json'))
    assert all(manifest[key] == value for key, value in expected.items())
    assert get(f'{base}/brands/{theme}/{manifest["logo"]}') == (fixtures / client / 'logo.png').read_bytes()
    print(f'{client}: discovery, JWKS, branded login, CSS, logo and public manifest verified')

assert b'LIVE' in get(base + '/healthz')
for path in ['/admin/', '/config_dump', '/brands/../installed.json', '/brands/',
             '/realms/master/.well-known/openid-configuration']:
    try:
        get(base + path)
        raise AssertionError('Unexpected public route: ' + path)
    except urllib.error.HTTPError as error:
        assert error.code == 404, (path, error.code)
print('Readiness and rejected private/unknown routes verified')
