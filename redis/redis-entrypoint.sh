#!/bin/sh
set -eu

PASSWORD_FILE="${REDIS_PASSWORD_FILE:-/run/secrets/redis_password}"
ACL_FILE="${REDIS_ACL_FILE:-/data/users.acl}"
REDIS_CONF="${REDIS_CONF:-/usr/local/etc/redis/redis.conf}"

if [ ! -r "$PASSWORD_FILE" ]; then
  echo "Redis password secret not found at: $PASSWORD_FILE" >&2
  exit 1
fi

PASSWORD="$(cat "$PASSWORD_FILE")"

# Ensure the data directory exists
mkdir -p /data

# Write ACL file atomically
TMP_ACL="${ACL_FILE}.tmp"
cat > "$TMP_ACL" <<EOF
user default on >${PASSWORD} ~* +@all
EOF
mv "$TMP_ACL" "$ACL_FILE"

exec "$@"
