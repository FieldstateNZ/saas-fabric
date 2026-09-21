storage "file" {
  path = "/openbao/file"
}
listener "tcp" {
  address = "0.0.0.0:8200"
  tls_disable = true
}
# Development storage and automatic unseal are local-only.
disable_mlock = true
ui = true
api_addr = "http://fabric-openbao:8200"
