"""OpenTofu-owned OCI brand installation. Requires Python 3 and ORAS on PATH."""
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
from oci_artifact import Artifact
from login_template import load_template
import sys
import tempfile

ARTIFACT_TYPE = "application/vnd.saas-fabric.brand.v1"


def validate_brand(data, logo):
    if set(data) != {"schemaVersion", "name", "primaryColor", "backgroundColor", "foregroundColor"}:
        raise ValueError("Unexpected brand manifest fields")
    if data["schemaVersion"] != 1 or not isinstance(data["name"], str) or not 1 <= len(data["name"]) <= 120:
        raise ValueError("Invalid brand schema or name")
    for key in ("primaryColor", "backgroundColor", "foregroundColor"):
        if not isinstance(data[key], str) or not re.fullmatch(r"#[a-fA-F0-9]{6}", data[key]):
            raise ValueError("Brand colours must be six-digit hex values")
    if len(logo) < 24 or logo[:8] != b"\x89PNG\r\n\x1a\n" or logo[12:16] != b"IHDR":
        raise ValueError("Logo must be a PNG")
    if any(n == 0 or n > 2048 for n in struct.unpack(">II", logo[16:24])):
        raise ValueError("Logo dimensions must be between 1 and 2048 pixels")


def verify(destination, theme, artifact, template=None):
    marker = destination / theme / "installed.json"
    data = json.loads(marker.read_text())
    if data["artifact"] != artifact or data.get("template") != template:
        raise ValueError("Installed brand reference differs")
    for relative, digest in data["files"].items():
        path = destination / relative
        if path.is_symlink() or not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError("Installed brand drift: " + relative)


def install(reference, destination, theme, plain_http, template=None, approved=(), keycloak_version="26.6"):
    extra, styles = load_template(template, approved, keycloak_version) if template else ({}, [])
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="brand-") as temporary:
        artifact = Artifact(reference, temporary, plain_http, ARTIFACT_TYPE)
        if set(artifact.layers) != {"brand.json", "logo.png"}:
            raise ValueError("Expected exactly brand.json and logo.png")
        data = json.loads(artifact.read("brand.json", "application/json"))
        logo = artifact.read("logo.png", "image/png")
        validate_brand(data, logo)
        css = Path(__file__).with_name("brand.css").read_text()
        for token, key in [("PRIMARY", "primaryColor"), ("BACKGROUND", "backgroundColor"), ("FOREGROUND", "foregroundColor")]:
            css = css.replace(token, data[key])
        files = {
            f"{theme}/login/theme.properties": b"parent=keycloak\nimport=common/keycloak\nstyles=css/login.css css/brand.css\n",
            f"{theme}/login/resources/css/brand.css": css.encode(),
            f"{theme}/login/resources/img/logo.png": logo,
            f"public/{theme}/brand.json": json.dumps({**data, "logo": "logo.png"}).encode(),
            f"public/{theme}/logo.png": logo,
        }
        files.update({f"{theme}/{path}": content for path, content in extra.items()})
        files[f"{theme}/login/theme.properties"] = (
            "parent=keycloak\nimport=common/keycloak\nstyles=css/login.css css/brand.css " + " ".join(styles) + "\n"
        ).encode()
        # No archive extraction. Trusted templates never go into the public asset directory.
        for relative, content in files.items():
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            pending = target.with_suffix(target.suffix + ".pending")
            pending.write_bytes(content)
            pending.chmod(0o644)
            os.replace(pending, target)
        marker = destination / theme / "installed.json"
        marker.write_text(json.dumps({"artifact": reference, "template": template, "files": {p: hashlib.sha256(c).hexdigest() for p, c in files.items()}}))
        marker.chmod(0o644)
        for path in destination.rglob("*"):
            if path.is_dir():
                path.chmod(0o755)
        destination.chmod(0o755)
    verify(destination, theme, reference, template)


def main():
    reference = os.environ["BRAND_ARTIFACT"]
    destination = Path(os.environ["BRAND_DESTINATION"])
    theme = os.environ["BRAND_THEME"]
    if not re.fullmatch(r"fabric-[a-z0-9-]+", theme) or not destination.is_absolute():
        raise ValueError("Invalid brand destination or theme name")
    template = json.loads(os.environ.get("BRAND_TEMPLATE", "null"))
    if "--verify" in sys.argv:
        verify(destination, theme, reference, template)
    else:
        install(reference, destination, theme, os.environ.get("BRAND_PLAIN_HTTP") == "true", template,
                json.loads(os.environ.get("APPROVED_LOGIN_TEMPLATES", "[]")), os.environ.get("KEYCLOAK_THEME_VERSION", "26.6"))
    print("Brand assets verified")


if __name__ == "__main__":
    main()
