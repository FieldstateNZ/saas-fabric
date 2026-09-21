terraform {
  required_version = ">= 1.12.0, < 2.0.0"
}
locals {
  renderer = sha256(join(":", concat([filesha256("${path.module}/brand.css")],
  [for file in sort(tolist(fileset(path.module, "*.py"))) : filesha256("${path.module}/${file}")])))
  revision   = substr(sha256(jsonencode([var.artifact, var.login_template, var.keycloak_version, local.renderer])), 0, 20)
  theme_name = "fabric-${var.client_id}-${local.revision}"
}
resource "terraform_data" "brand" {
  triggers_replace = [var.artifact, local.theme_name, var.destination, var.plain_http]
  lifecycle {
    precondition {
      condition     = var.login_template == null ? true : contains(var.approved_login_templates, var.login_template.artifact)
      error_message = "Login template must be explicitly approved by the platform deployment policy."
    }
  }
  provisioner "local-exec" {
    command     = "python3 materialize.py"
    working_dir = path.module
    environment = {
      BRAND_ARTIFACT           = var.artifact
      BRAND_DESTINATION        = var.destination
      BRAND_THEME              = local.theme_name
      BRAND_PLAIN_HTTP         = tostring(var.plain_http)
      BRAND_TEMPLATE           = jsonencode(var.login_template)
      APPROVED_LOGIN_TEMPLATES = jsonencode(var.approved_login_templates)
      KEYCLOAK_THEME_VERSION   = var.keycloak_version
    }
  }
}
output "theme_name" {
  value      = local.theme_name
  depends_on = [terraform_data.brand]
}
output "asset_path" {
  value      = "/brands/${local.theme_name}/brand.json"
  depends_on = [terraform_data.brand]
}
