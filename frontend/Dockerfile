# syntax=docker/dockerfile:1.4

FROM node:24-alpine@sha256:a0b9bf06e4e6193cf7a0f58816cc935ff8c2a908f81e6f1a95432d679c54fbfd AS base

FROM base AS deps
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci

FROM base AS development
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .
ENTRYPOINT ["npm", "run", "dev", "--", "--host", "0.0.0.0", "--port", "5179"]

FROM base AS builder
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .
RUN npm run build

FROM nginx:1.29-alpine@sha256:5616878291a2eed594aee8db4dade5878cf7edcb475e59193904b198d9b830de

RUN addgroup -S -g 1001 app && adduser -S -u 1001 -G app -H app \
    && mkdir -p /app/www /var/cache/nginx /var/log/nginx \
    && chown -R 1001:1001 /var/cache/nginx /var/log/nginx

COPY --chown=1001:1001 --from=builder /app/dist /app/www
COPY --chown=1001:1001 --from=builder /app/nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget -qO- http://localhost:8080/ || exit 1

USER 1001

CMD ["nginx", "-g", "daemon off;"]
