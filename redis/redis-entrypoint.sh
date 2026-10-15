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

# Write ACL file atomically; it holds the password, so keep it owner-only.
umask 077
TMP_ACL="${ACL_FILE}.tmp"
cat > "$TMP_ACL" <<ACL
user default on >${PASSWORD} ~* +@all
ACL
mv "$TMP_ACL" "$ACL_FILE"

# Hand the data directory to the redis user, then let the stock entrypoint drop
# root before starting the server. The stock permission fixer skips /data
# because of users.acl, so ownership is set here instead.
if [ "$(id -u)" = 0 ]; then
  chown -R redis:redis /data
  export SKIP_FIX_PERMS=1
fi

exec docker-entrypoint.sh "$@"
