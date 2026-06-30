# syntax=docker/dockerfile:1.4

ARG NODE_VERSION=23
ARG PORT=3000

FROM node:${NODE_VERSION}-alpine AS base

# Install dependencies only when needed
FROM base as deps

WORKDIR /app

COPY package.json package-lock.json ./

RUN npm ci


# Rebuild the source code only when needed
FROM base AS builder

WORKDIR /app

COPY --from=deps /app/node_modules ./node_modules

COPY . .

# Build the app
RUN npm run build

# Serve stage
FROM nginx:alpine

RUN addgroup --system --gid 1001 nodejs
RUN adduser --system --uid 1001 nginx

RUN mkdir -p /app/www

COPY --from=builder /app/dist /app/www

COPY --from=builder /app/.nginx/nginx.conf /etc/nginx/conf.d/default.conf


RUN chown -R nginx:nodejs /app/www && \
    chown -R nginx:nodejs /var/cache/nginx && \
    chown -R nginx:nodejs /var/log/nginx && \
    chown -R nginx:nodejs /etc/nginx/conf.d && \
    chown -R nginx:nodejs /var/run/nginx.pid

# Expose port
EXPOSE 80

# Tell nginx's template processing which variables to replace
ENV NGINX_ENVSUBST_TEMPLATE_DIR=/etc/nginx/templates
ENV NGINX_ENVSUBST_TEMPLATE_SUFFIX=.template
ENV NGINX_ENVSUBST_OUTPUT_DIR=/etc/nginx/conf.d
ENV PORT=80

# Switch to non-root user
USER nginx

# Start nginx
CMD ["nginx", "-g", "daemon off;"]




FROM nginx:alpine

# Copy config nginx
COPY --from=build /app/.nginx/nginx.conf /etc/nginx/conf.d/default.conf

WORKDIR /usr/share/nginx/html

# Remove default nginx static assets
RUN rm -rf ./*

# Copy static assets from builder stage
COPY --from=build /app/build .

# Containers run nginx with global directives and daemon off
ENTRYPOINT ["nginx", "-g", "daemon off;"]