import hashlib
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'infrastructure/opentofu/modules/client-brand'))
from login_template import load_template, ARTIFACT_TYPE
import oci_artifact


class TemplateTests(unittest.TestCase):
    def package(self, version='26.6', extra=None):
        fixture = ROOT / 'examples/login-templates/welcome-panel'
        metadata = json.loads((fixture / 'template.json').read_text())
        metadata['keycloakVersion'] = version
        files = {name: (fixture / name).read_bytes() for name in metadata['files']}
        if extra:
            metadata['files'][extra] = 'text/plain'
            files[extra] = b'unexpected'
        files['template.json'] = json.dumps(metadata).encode()
        blobs, layers = {}, []
        for name, content in files.items():
            digest = 'sha256:' + hashlib.sha256(content).hexdigest()
            blobs[digest] = content
            layers.append(dict(digest=digest, size=len(content),
                               mediaType='application/json' if name == 'template.json' else metadata['files'][name],
                               annotations={'org.opencontainers.image.title': name}))
        manifest = json.dumps(dict(artifactType=ARTIFACT_TYPE, layers=layers)).encode()
        reference = 'registry.example/templates/panel@sha256:' + hashlib.sha256(manifest).hexdigest()

        def fetch(args, **kwargs):
            target = Path(args[args.index('--output') + 1])
            target.write_bytes(manifest if args[1] == 'manifest' else blobs[args[-1].split('@')[1]])
        return dict(artifact=reference, plain_http=False), fetch

    def test_approved_package_preserves_real_template_and_messages(self):
        selection, fetch = self.package()
        with patch.object(oci_artifact.subprocess, 'run', fetch):
            files, styles = load_template(selection, [selection['artifact']], '26.6')
        self.assertIn(b'${realm.displayName!realm.name}', files['login/footer.ftl'])
        self.assertIn('login/messages/messages_en.properties', files)
        self.assertEqual(['css/template.css'], styles)
        self.assertFalse(any(path.startswith('public/') for path in files))

    def test_unapproved_template_is_not_downloaded(self):
        selection, _ = self.package()
        with patch.object(oci_artifact.subprocess, 'run') as fetch:
            with self.assertRaisesRegex(ValueError, 'approved'):
                load_template(selection, [], '26.6')
            fetch.assert_not_called()

    def test_wrong_keycloak_version_is_rejected(self):
        selection, fetch = self.package(version='27.0')
        with patch.object(oci_artifact.subprocess, 'run', fetch):
            with self.assertRaisesRegex(ValueError, 'incompatible'):
                load_template(selection, [selection['artifact']], '26.6')

    def test_rejects_paths_outside_template_contract(self):
        for path in ['../login.ftl', 'login/../../secret', 'login/theme.properties',
                     'login/resources/js/code.js', 'public/login.ftl']:
            selection, fetch = self.package(extra=path)
            with self.subTest(path=path), patch.object(oci_artifact.subprocess, 'run', fetch):
                with self.assertRaises(ValueError):
                    load_template(selection, [selection['artifact']], '26.6')

    def test_rejects_wrong_manifest_digest(self):
        selection, fetch = self.package()
        selection['artifact'] = selection['artifact'].split('@')[0] + '@sha256:' + '0' * 64
        with patch.object(oci_artifact.subprocess, 'run', fetch):
            with self.assertRaisesRegex(ValueError, 'digest mismatch'):
                load_template(selection, [selection['artifact']], '26.6')


if __name__ == '__main__':
    unittest.main()
