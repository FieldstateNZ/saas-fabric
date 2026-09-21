#!/bin/sh
set -eu
umask 077
# Only bootstrap/unseal here. OpenTofu owns namespaces, mounts and policies.
status() { bao status 2>/dev/null || test "$?" = 2; }
field() { status | awk -v key="$1" '$1 == key { print $2 }'; }
initialized="$(field Initialized)"
case "$initialized" in
  false)
    if [ -s /keys/init.txt ]; then
      echo 'OpenBao data is missing but keys remain; refusing to replace the keys.' >&2
      exit 1
    fi
    bao operator init -key-shares=1 -key-threshold=1 > /keys/init.txt
    ;;
  true) ;;
  *) echo 'OpenBao did not return initialization status.' >&2; exit 1 ;;
esac
if [ ! -s /keys/init.txt ]; then
  echo 'OpenBao is initialized but its local unseal keys are missing.' >&2
  exit 1
fi
if [ "$(field Sealed)" = true ]; then
  key="$(awk '/^Unseal Key 1:/ {print $4}' /keys/init.txt)"
  bao operator unseal "$key" >/dev/null
fi
bao status >/dev/null
echo 'OpenBao initialized and unsealed. Local keys retained in its private volume.'
