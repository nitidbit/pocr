FROM nginx:1.29-alpine@sha256:5616878291a2eed594aee8db4dade5878cf7edcb475e59193904b198d9b830de

# Defaults – can be overridden by docker compose
ENV NGINX_SERVER_PORT=443 \
    NGINX_GRPC_PORT=1443 \
    NGINX_SERVER_NAME=localhost

COPY nginx.conf /etc/nginx/nginx.conf
COPY snippets/ /etc/nginx/snippets/
COPY templates/ /etc/nginx/templates/
