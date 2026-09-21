"""Bounded OCI retrieval shared by data-only brands and trusted template packages."""
import hashlib
import json
from pathlib import Path
import re
import subprocess

REFERENCE = r"[a-z0-9][a-z0-9.:-]*/[a-z0-9/_-]+@sha256:[a-f0-9]{64}"


class Artifact:
    def __init__(self, reference, stage, plain_http, artifact_type):
        if not re.fullmatch(REFERENCE, reference):
            raise ValueError("A digest-pinned OCI reference is required")
        self.reference, self.stage = reference, Path(stage)
        self.flags = ["--plain-http"] if plain_http else []
        manifest_file = self.stage / "manifest.json"
        subprocess.run(["oras", "manifest", "fetch", *self.flags, "--output", str(manifest_file), reference], check=True)
        raw = manifest_file.read_bytes()
        if hashlib.sha256(raw).hexdigest() != reference.split("@sha256:")[1]:
            raise ValueError("OCI manifest digest mismatch")
        manifest = json.loads(raw)
        layers = manifest.get("layers", [])
        if manifest.get("artifactType") != artifact_type or not 1 <= len(layers) <= 32:
            raise ValueError("Unexpected OCI artifact type or layer count")
        self.layers = {}
        for layer in layers:
            name = layer.get("annotations", {}).get("org.opencontainers.image.title")
            if not isinstance(name, str) or name in self.layers:
                raise ValueError("Missing or duplicate OCI layer title")
            if not isinstance(layer.get("size"), int) or not 0 < layer['size'] <= 2 * 1024 * 1024:
                raise ValueError("OCI layer exceeds size limit")
            if not re.fullmatch(r"sha256:[a-f0-9]{64}", layer.get('digest', '')):
                raise ValueError("Invalid blob digest")
            self.layers[name] = layer
        if sum(layer['size'] for layer in layers) > 16 * 1024 * 1024:
            raise ValueError("OCI package exceeds size limit")

    def read(self, name, media_type):
        layer = self.layers.get(name)
        if layer is None or layer.get('mediaType') != media_type:
            raise ValueError("Missing layer or unexpected media type: " + name)
        # Filenames from manifests are never used as local download paths.
        target = self.stage / layer['digest'][7:]
        subprocess.run(["oras", "blob", "fetch", *self.flags, "--output", str(target),
                        self.reference.split('@')[0] + '@' + layer['digest']], check=True)
        content = target.read_bytes()
        if len(content) != layer['size'] or hashlib.sha256(content).hexdigest() != layer['digest'][7:]:
            raise ValueError("OCI blob integrity mismatch")
        return content
