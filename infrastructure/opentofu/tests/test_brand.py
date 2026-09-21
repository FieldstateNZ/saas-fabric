import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
MODULE = ROOT / 'infrastructure/opentofu/modules/client-brand/materialize.py'
sys.path.insert(0, str(MODULE.parent))
spec = importlib.util.spec_from_file_location('brand', MODULE)
brand = importlib.util.module_from_spec(spec)
spec.loader.exec_module(brand)


class BrandTests(unittest.TestCase):
    def setUp(self):
        self.data = json.loads((ROOT / 'examples/brands/demo/brand.json').read_text())
        self.logo = (ROOT / 'examples/brands/demo/logo.png').read_bytes()

    def test_valid_fixture(self):
        brand.validate_brand(self.data, self.logo)

    def test_rejects_active_colour_content(self):
        self.data['primaryColor'] = 'url(https://example.test/track)'
        with self.assertRaises(ValueError):
            brand.validate_brand(self.data, self.logo)

    def test_rejects_client_templates(self):
        self.data['template'] = '<script>unexpected</script>'
        with self.assertRaises(ValueError):
            brand.validate_brand(self.data, self.logo)

    def test_rejects_non_png(self):
        with self.assertRaises(ValueError):
            brand.validate_brand(self.data, b'<svg/>')

    def artifact(self, title='brand.json'):
        blobs = {}
        layers = []
        for name, media, content in [(title, 'application/json', json.dumps(self.data).encode()),
                                     ('logo.png', 'image/png', self.logo)]:
            digest = 'sha256:' + hashlib.sha256(content).hexdigest()
            blobs[digest] = content
            layers.append(dict(mediaType=media, size=len(content), digest=digest,
                               annotations={'org.opencontainers.image.title': name}))
        manifest = json.dumps(dict(artifactType=brand.ARTIFACT_TYPE, layers=layers)).encode()
        reference = 'registry.example/brand@sha256:' + hashlib.sha256(manifest).hexdigest()

        def fetch(args, **kwargs):
            target = Path(args[args.index('--output') + 1])
            target.write_bytes(manifest if args[1] == 'manifest' else blobs[args[-1].split('@')[1]])
        return reference, fetch

    def test_installs_isolated_versions_and_detects_drift(self):
        reference, fetch = self.artifact()
        with tempfile.TemporaryDirectory() as directory, patch.object(brand.subprocess, 'run', fetch):
            destination = Path(directory)
            for theme in ['fabric-one-123', 'fabric-two-456']:
                brand.install(reference, destination, theme, False)
                brand.verify(destination, theme, reference)
            css = destination / 'fabric-one-123/login/resources/css/brand.css'
            self.assertIn(self.data['primaryColor'], css.read_text())
            css.write_text('changed')
            with self.assertRaises(ValueError):
                brand.verify(destination, 'fabric-one-123', reference)
            brand.verify(destination, 'fabric-two-456', reference)
            brand.install(reference, destination, 'fabric-one-123', False)
            brand.verify(destination, 'fabric-one-123', reference)

    def test_rejects_traversal_before_fetching_layers(self):
        reference, fetch = self.artifact('../brand.json')
        with tempfile.TemporaryDirectory() as directory, patch.object(brand.subprocess, 'run', fetch):
            with self.assertRaises(ValueError):
                brand.install(reference, Path(directory), 'fabric-demo-123', False)
            self.assertEqual([], list(Path(directory).iterdir()))

    def test_rejects_floating_tags(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                brand.install('registry.example/brand:latest', Path(directory), 'fabric-demo-123', False)


if __name__ == '__main__':
    unittest.main()
