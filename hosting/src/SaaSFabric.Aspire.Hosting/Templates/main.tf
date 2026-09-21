terraform {
  required_version = ">= 1.12.0, < 2.0.0"
  required_providers {
    keycloak = {
      source  = "keycloak/keycloak"
      version = "5.9.0"
    }
    vault = {
      source  = "hashicorp/vault"
      version = "5.11.0"
    }
  }
  backend "local" {}
}
# Provider-native environment variables carry local endpoints and credentials.
provider "keycloak" {}
provider "vault" {
  skip_child_token = true
}
resource "keycloak_realm" "client" {
  realm        = var.realm
  display_name = var.display_name
  enabled      = true
  login_theme  = var.brand == null ? null : module.brand[0].theme_name
  # Realm identity is not renamed/deleted automatically by a config typo.
  lifecycle { prevent_destroy = true }
}
resource "keycloak_role" "roles" {
  for_each = setunion(var.roles, ["Client Realm Administrator", "Client Realm User"])
  realm_id = keycloak_realm.client.id
  name     = each.value
}
resource "keycloak_openid_client" "applications" {
  for_each                        = var.applications
  realm_id                        = keycloak_realm.client.id
  client_id                       = each.key
  access_type                     = "PUBLIC"
  standard_flow_enabled           = true
  implicit_flow_enabled           = false
  direct_access_grants_enabled     = false
  service_accounts_enabled        = false
  pkce_code_challenge_method       = "S256"
  valid_redirect_uris             = each.value.redirect_uris
  valid_post_logout_redirect_uris  = ["+"]
  web_origins                     = each.value.web_origins
}
resource "keycloak_openid_audience_protocol_mapper" "audience" {
  for_each                 = keycloak_openid_client.applications
  realm_id                 = keycloak_realm.client.id
  client_id                = each.value.id
  name                     = "fabric-audience"
  included_custom_audience = var.audience
  add_to_access_token      = true
  add_to_id_token          = false
}
resource "vault_namespace" "client" {
  path = var.client_id
  lifecycle { prevent_destroy = true }
}
resource "vault_mount" "secrets" {
  namespace = vault_namespace.client.path_fq
  path      = "secret"
  type      = "kv"
  options   = { version = "2" }
}
resource "vault_policy" "client" {
  namespace = vault_namespace.client.path_fq
  name      = "client"
  policy    = <<-POLICY
    path "secret/data/*" { capabilities = ["create", "read", "update", "delete"] }
    path "secret/metadata/*" { capabilities = ["read", "list", "delete"] }
  POLICY
}
