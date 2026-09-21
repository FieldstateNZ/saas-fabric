variable "client_id" {
  type = string
  validation {
    condition     = can(regex("^[a-z][a-z0-9-]{0,47}$", var.client_id))
    error_message = "Use a lowercase client ID."
  }
}
variable "artifact" {
  description = "OCI registry repository pinned to a sha256 manifest digest; no credentials."
  type        = string
  validation {
    condition     = can(regex("^[a-z0-9][a-z0-9.:-]*/[a-z0-9/_-]+@sha256:[a-f0-9]{64}$", var.artifact))
    error_message = "Brand must be an OCI repository reference pinned by sha256 digest."
  }
}
variable "destination" {
  description = "Persistent writable directory shared with Keycloak and the asset server (public/)."
  type        = string
  validation {
    condition     = startswith(var.destination, "/")
    error_message = "Use an absolute mounted directory."
  }
}
variable "login_template" {
  description = "Optional separately published, approved login-template OCI artifact."
  type = object({
    artifact   = string
    plain_http = optional(bool, false)
  })
  default = null
  validation {
    condition = var.login_template == null ? true : can(regex(
    "^[a-z0-9][a-z0-9.:-]*/[a-z0-9/_-]+@sha256:[a-f0-9]{64}$", var.login_template.artifact))
    error_message = "Login template must be pinned by SHA-256 digest."
  }
}
variable "approved_login_templates" {
  description = "Platform-owned exact reference allowlist, supplied independently of client configuration."
  type        = set(string)
  default     = []
}
variable "keycloak_version" {
  description = "Target Keycloak major.minor compatibility line. Package metadata must match."
  type        = string
  default     = "26.6"
}
variable "plain_http" {
  description = "Explicit opt-in for a local HTTP registry. TLS remains the default."
  type        = bool
  default     = false
}
