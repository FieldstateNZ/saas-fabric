FROM ghcr.io/opentofu/opentofu:1.12.0-minimal AS tofu
FROM ghcr.io/oras-project/oras:v1.3.4 AS oras
FROM alpine:3.23
RUN apk add --no-cache ca-certificates curl jq python3
COPY --from=tofu /usr/local/bin/tofu /usr/local/bin/tofu
COPY --from=oras /bin/oras /usr/local/bin/oras
COPY *.tf /module/
COPY template-policy.json /module/
COPY brand-module /module/brand-module/
COPY apply.sh verify.sh /scripts/
ENTRYPOINT ["/bin/sh", "/scripts/apply.sh"]
