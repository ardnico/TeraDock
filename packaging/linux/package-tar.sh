#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
if [[ -z "${VERSION}" ]]; then
  echo "Usage: $0 <version>" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"
STAGING_DIR="${ROOT_DIR}/dist/td-${VERSION}-linux-x86_64"

mkdir -p "${DIST_DIR}"
rm -rf "${STAGING_DIR}"
mkdir -p "${STAGING_DIR}"

cp "${ROOT_DIR}/target/release/td" "${STAGING_DIR}/td"

(
  cd "${DIST_DIR}"
  tar -czf "td-${VERSION}-linux-x86_64.tar.gz" "td-${VERSION}-linux-x86_64"
)

echo "Created ${DIST_DIR}/td-${VERSION}-linux-x86_64.tar.gz"
