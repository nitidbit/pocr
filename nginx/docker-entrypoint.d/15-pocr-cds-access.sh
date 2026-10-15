#!/bin/sh
# Writes /etc/nginx/snippets/cds-access.conf from CDS_ALLOW_FROM: who may call
# the CDS Hooks endpoints (/cds-services). Runs before the stock template step.
#
#   CDS_ALLOW_FROM="10.20.30.40 10.20.31.0/24"   only these addresses/ranges
#   CDS_ALLOW_FROM=all                           anyone (demo servers only)
#   unset or empty                               no one (fail closed)
#
# Separate entries with spaces or commas. See docs/security-concerns.md S2.
set -euf   # -f: no globbing of the entries below

out=/etc/nginx/snippets/cds-access.conf
entries=$(printf '%s' "${CDS_ALLOW_FROM:-}" | tr ',' ' ')

{
  echo "# Generated at container start from CDS_ALLOW_FROM; do not edit."
  if [ -z "$(echo $entries)" ]; then
    echo "deny all;"
  elif [ "$(echo $entries)" = "all" ]; then
    echo "allow all;"
  else
    for e in $entries; do
      case "$e" in
        *[!0-9A-Fa-f.:/]*)
          echo "15-pocr-cds-access: invalid CDS_ALLOW_FROM entry '$e'" >&2
          exit 1 ;;
      esac
      echo "allow $e;"
    done
    echo "deny all;"
  fi
} > "$out.tmp"
mv "$out.tmp" "$out"

if [ -z "$(echo $entries)" ]; then
  echo "15-pocr-cds-access: CDS_ALLOW_FROM is empty; /cds-services will refuse every caller" >&2
fi
