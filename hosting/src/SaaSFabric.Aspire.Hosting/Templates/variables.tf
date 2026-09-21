variable "client_id" { type = string }
variable "realm" { type = string }
variable "display_name" { type = string }
variable "audience" { type = string }
variable "roles" { type = set(string) }
variable "applications" {
  type = map(object({
    redirect_uris = list(string)
    web_origins   = list(string)
  }))
}
