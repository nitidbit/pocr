# syntax=docker/dockerfile:1.4

FROM node:24-alpine AS base

FROM base AS deps
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci

FROM base AS development
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
ENTRYPOINT ["sh", "-c", "npm install && exec npm run dev -- --host 0.0.0.0 --port 5179"]

FROM base AS builder
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .
RUN npm run build

FROM nginx:1.29-alpine

RUN addgroup -S app && adduser -S -G app app \
    && mkdir -p /app/www /var/cache/nginx /var/log/nginx \
    && chown -R app:app /var/cache/nginx /var/log/nginx

COPY --chown=app:app --from=builder /app/dist /app/www
COPY --chown=app:app --from=builder /app/nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget -qO- http://localhost:8080/ || exit 1

USER app

CMD ["nginx", "-g", "daemon off;"]
