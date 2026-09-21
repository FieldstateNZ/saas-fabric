# OCI brand fixtures

`demo` is blue and `contrast` is copper. These are fictional local fixtures.
Publish them independently of Aspire with ORAS 1.3.4:

```sh
./examples/brands/publish.sh localhost:REGISTRY_PORT --plain-http
```

Use the local registry URL shown in the demo dashboard. The script fixes the
creation annotation so re-publishing unchanged files produces the same digest.
Copy each reported digest into the client YAML:

```yaml
brand:
  artifact: fabric-brand-registry:5000/brands/demo@sha256:DIGEST
  plainHttp: true
```

That hostname is the demo's container-network alias, not the host's publish URL.
Use your real TLS registry hostname outside the demo and omit `plainHttp`.
Restart the isolated AppHost after changing YAML. OpenTofu performs deployment;
Aspire does not pull, render, validate or select brands.

On a fresh machine, start the demo first. The registry becomes healthy even
though the client applies fail because their artifacts have not been published
yet. Publish these fixtures to its allocated host URL, then restart the isolated
AppHost. The committed fixture digests match the deterministic publishing script.
Registry and brand storage then persist across restarts. Artifact publishing is deliberately
separate from applying client configuration.

Verify both brands, login assets, discovery/JWKS and private-route rejection:

```sh
python3 examples/brands/verify.py http://localhost:GATEWAY_PORT
python3 -m unittest discover -s infrastructure/opentofu/tests -v
```

These checks load the login forms without signing in or creating any users.
