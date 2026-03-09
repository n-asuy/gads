#!/bin/bash
set -e

CRATE_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$CRATE_DIR"

# Load .env if exists
if [ -f .env ]; then
  export $(grep -v '^#' .env | xargs)
fi

# Increment patch version in Cargo.toml
CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
MAJOR=$(echo "$CURRENT" | cut -d. -f1)
MINOR=$(echo "$CURRENT" | cut -d. -f2)
PATCH=$(echo "$CURRENT" | cut -d. -f3)
NEW_PATCH=$((PATCH + 1))
NEW_VERSION="${MAJOR}.${MINOR}.${NEW_PATCH}"

echo "=== Version bump: ${CURRENT} -> ${NEW_VERSION} ==="
sed -i '' "s/^version = \"${CURRENT}\"/version = \"${NEW_VERSION}\"/" Cargo.toml

# Build + pack
bash local-build.sh

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
TGZ_FILE="npx-cli/gads-${VERSION}.tgz"

if [ ! -f "$TGZ_FILE" ]; then
  echo "Error: $TGZ_FILE not found"
  exit 1
fi

# Upload to R2 (if configured)
if [ -n "$R2_ENDPOINT" ] && [ -n "$R2_BUCKET" ] && [ -n "$R2_PUBLIC_URL" ]; then
  echo "=== Uploading platform binaries to R2 ==="

  for platform_zip in npx-cli/dist/*/gads.zip; do
    if [ -f "$platform_zip" ]; then
      platform_dir=$(basename "$(dirname "$platform_zip")")
      s3_path="s3://$R2_BUCKET/releases/v${VERSION}/${platform_dir}/gads.zip"
      echo "  Uploading ${platform_dir}/gads.zip -> ${s3_path}"
      aws s3 cp "$platform_zip" "$s3_path" --endpoint-url "$R2_ENDPOINT"
    fi
  done

  aws s3 cp "$TGZ_FILE" \
    "s3://$R2_BUCKET/releases/gads-${VERSION}.tgz" \
    --endpoint-url "$R2_ENDPOINT"

  echo "{\"latest\": \"$VERSION\"}" | aws s3 cp - \
    "s3://$R2_BUCKET/releases/latest.json" \
    --endpoint-url "$R2_ENDPOINT" \
    --content-type "application/json"

  echo ""
  echo "R2 URLs:"
  for platform_zip in npx-cli/dist/*/gads.zip; do
    if [ -f "$platform_zip" ]; then
      platform_dir=$(basename "$(dirname "$platform_zip")")
      echo "  $R2_PUBLIC_URL/releases/v${VERSION}/${platform_dir}/gads.zip"
    fi
  done
else
  echo "(Skipping R2 — env not set)"
fi

# Publish to npm
echo ""
echo "=== Publishing to npm ==="
npm publish "./$TGZ_FILE" || {
  echo "(npm publish failed — run manually: npm publish ./$TGZ_FILE)"
}

echo ""
echo "=== Release complete ==="
echo "  npm install -g gads"
