variable "brand" {
  type = object({
    artifact   = string
    plain_http = optional(bool, false)
  })
  default = null
}
module "brand" {
  count                    = var.brand == null ? 0 : 1
  source                   = "./brand-module"
  client_id                = var.client_id
  artifact                 = var.brand.artifact
  plain_http               = var.brand.plain_http
  destination              = "/brands"
  login_template           = var.login_template
  approved_login_templates = jsondecode(file("${path.module}/template-policy.json"))
}
variable "login_template" {
  type = object({
    artifact   = string
    plain_http = optional(bool, false)
  })
  default = null
  validation {
    condition     = var.login_template == null || var.brand != null
    error_message = "A login template requires a brand selection."
  }
}
output "brand_theme" {
  value = var.brand == null ? "" : module.brand[0].theme_name
}
output "brand_asset_path" {
  value = var.brand == null ? "" : module.brand[0].asset_path
}
