#!/bin/sh
set -eu
umask 077
export TF_IN_AUTOMATION=1 TF_INPUT=0
export VAULT_TOKEN="$(awk '/^Initial Root Token:/ {print $4}' /keys/init.txt)"
test -n "$VAULT_TOKEN"
mkdir -p /state/module
cp /module/*.tf /state/module/
cp /module/template-policy.json /state/module/
cp -R /module/brand-module /state/module/
tofu -chdir=/state/module init -input=false -no-color -backend-config=path=/state/terraform.tfstate
tofu -chdir=/state/module validate -no-color
tofu -chdir=/state/module apply -input=false -auto-approve -no-color
brand_theme="$(tofu -chdir=/state/module output -raw brand_theme)"
if [ -n "$brand_theme" ]; then
  export BRAND_ARTIFACT="$(printf '%s' "$TF_VAR_brand" | jq -r .artifact)"
  export BRAND_DESTINATION=/brands BRAND_THEME="$brand_theme"
  export BRAND_TEMPLATE="$TF_VAR_login_template"
  python3 /state/module/brand-module/materialize.py --verify
fi
# Exit 2 means drift still exists; do not report a successful reconciliation.
tofu -chdir=/state/module plan -input=false -no-color -detailed-exitcode
/bin/sh /scripts/verify.sh
echo "Client $TF_VAR_client_id provisioned and verified; second plan has no changes."
