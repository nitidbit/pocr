FROM nginx:1.31-alpine

# Defaults – can be overridden by docker compose
ENV NGINX_SERVER_PORT=443 \
    NGINX_GRPC_PORT=1443 \
    NGINX_SERVER_NAME=localhost

COPY nginx.conf /etc/nginx/nginx.conf
COPY snippets/ /etc/nginx/snippets/
COPY templates/ /etc/nginx/templates/
