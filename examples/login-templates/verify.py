"""Exercise custom template isolation and inherited login behavior without demo users."""
import html
import re
import sys
import urllib.error
import urllib.parse
import urllib.request

base = sys.argv[1].rstrip('/')
query = urllib.parse.urlencode(dict(
    client_id='app-shell', redirect_uri='http://127.0.0.1:5186/callback',
    response_type='code', scope='openid', code_challenge='A' * 43,
    code_challenge_method='S256'))
for realm, custom in [('fabric-demo', False), ('fabric-contrast', True)]:
    opener = urllib.request.build_opener()
    url = f'{base}/realms/{realm}/protocol/openid-connect/auth?{query}'
    with opener.open(url, timeout=15) as response:
        assert response.status == 200
        page = response.read().decode()
    assert ('data-template="welcome-panel"' in page) == custom
    assert 'name="username"' in page and 'name="password"' in page
    action = html.unescape(re.search(r'<form[^>]*id="kc-form-login"[^>]*action="([^"]+)"', page)[1])
    assert urllib.parse.urlsplit(action).netloc == urllib.parse.urlsplit(base).netloc
    if custom:
        assert 'Your workspace starts here.' in page and 'Fabric Copper' in page
        theme = re.search(r'/login/(fabric-[^/]+)/css/template.css', page)[1]
        for path in [f'/brands/{theme}/login/footer.ftl', f'/brands/{theme}/installed.json']:
            try:
                opener.open(base + path, timeout=15)
                raise AssertionError('Template internals are public')
            except urllib.error.HTTPError as error:
                assert error.code == 404
    print(realm + ': correct template, inherited form/action and private-source isolation verified')
