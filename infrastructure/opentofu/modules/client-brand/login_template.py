"""Install only explicitly approved, compatible server-side theme packages."""
import json
import re
import tempfile
from oci_artifact import Artifact

ARTIFACT_TYPE = 'application/vnd.saas-fabric.login-template.v1'


def load_template(selection, approved, keycloak_version):
    reference = selection['artifact']
    if reference not in approved:
        raise ValueError('Login template is not in the platform-approved artifact list')
    with tempfile.TemporaryDirectory(prefix='login-template-') as stage:
        artifact = Artifact(reference, stage, selection.get('plain_http', False), ARTIFACT_TYPE)
        metadata = json.loads(artifact.read('template.json', 'application/json'))
        if set(metadata) != {'schemaVersion', 'name', 'keycloakVersion', 'parent', 'files'}:
            raise ValueError('Unexpected template manifest fields')
        if metadata['schemaVersion'] != 1 or metadata['keycloakVersion'] != keycloak_version:
            raise ValueError('Login template is incompatible with the configured Keycloak version')
        if metadata['parent'] != 'keycloak' or not re.fullmatch(r'[a-z][a-z0-9-]{0,63}', metadata['name']):
            raise ValueError('Invalid template name or parent')
        files = metadata['files']
        if not isinstance(files, dict) or not files or set(artifact.layers) != {'template.json', *files}:
            raise ValueError('Template file declaration differs from OCI layers')
        installed = {}
        styles = []
        for path, media_type in files.items():
            if re.fullmatch(r'login/[a-zA-Z0-9_-]+\.ftl', path):
                expected = 'text/plain'
            elif re.fullmatch(r'login/resources/css/[a-zA-Z0-9_-]+\.css', path):
                if path == 'login/resources/css/brand.css':
                    raise ValueError('Templates cannot replace brand tokens')
                expected = 'text/css'
                styles.append(path.removeprefix('login/resources/'))
            elif re.fullmatch(r'login/messages/messages_[a-zA-Z_-]+\.properties', path):
                expected = 'text/plain'
            else:
                raise ValueError('Unsupported template file path: ' + path)
            if media_type != expected:
                raise ValueError('Unexpected template file media type')
            content = artifact.read(path, media_type)
            content.decode('utf-8')
            installed[path] = content
        if not any(path.endswith('.ftl') for path in files):
            raise ValueError('A template package must contain a FreeMarker template')
        return installed, sorted(styles)
