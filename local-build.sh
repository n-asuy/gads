#!/bin/bash
set -e

CRATE_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$CRATE_DIR"

# Load .env if exists
if [ -f .env ]; then
  export $(grep -v '^#' .env | xargs)
fi

sync_r2_public_url() {
  if [ -z "${R2_PUBLIC_URL:-}" ]; then
    return
  fi

  local platform_js="npx-cli/bin/platform.js"
  if [ ! -f "$platform_js" ]; then
    return
  fi

  R2_PUBLIC_URL="$R2_PUBLIC_URL" perl -i -pe \
    's#const R2_PUBLIC_URL = ".*";#const R2_PUBLIC_URL = "$ENV{R2_PUBLIC_URL}";#' \
    "$platform_js"

  echo "Using R2_PUBLIC_URL: $R2_PUBLIC_URL"
}

sync_r2_public_url

# Detect host platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64) PLATFORM_DIR="macos-arm64" ;;
      x86_64) PLATFORM_DIR="macos-x64" ;;
      *) echo "Unsupported macOS arch: $ARCH"; exit 1 ;;
    esac
    BIN_NAME="gads"
    ;;
  Linux)
    case "$ARCH" in
      x86_64) PLATFORM_DIR="linux-x64" ;;
      aarch64) PLATFORM_DIR="linux-arm64" ;;
      *) echo "Unsupported Linux arch: $ARCH"; exit 1 ;;
    esac
    BIN_NAME="gads"
    ;;
  MINGW*|MSYS*|CYGWIN*)
    case "$ARCH" in
      x86_64|AMD64) PLATFORM_DIR="windows-x64" ;;
      aarch64|ARM64) PLATFORM_DIR="windows-arm64" ;;
      *) echo "Unsupported Windows arch: $ARCH"; exit 1 ;;
    esac
    BIN_NAME="gads.exe"
    ;;
  *)
    echo "Unsupported OS: $OS"
    exit 1
    ;;
esac

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

echo "=== gads local build ==="
echo "Version:  $VERSION"
echo "Platform: $PLATFORM_DIR"

echo "Cleaning previous builds..."
rm -rf npx-cli/dist
mkdir -p "npx-cli/dist/$PLATFORM_DIR"

echo "Building Rust binary..."
cargo build --release -p gads

echo "Creating distribution package..."
cp "../target/release/$BIN_NAME" "$BIN_NAME"
zip -q gads.zip "$BIN_NAME"
rm -f "$BIN_NAME"
mv gads.zip "npx-cli/dist/$PLATFORM_DIR/gads.zip"

echo "Packing npm package..."
cd npx-cli
npm version "$VERSION" --no-git-tag-version --allow-same-version 2>/dev/null || true
rm -f gads-*.tgz
npm pack --quiet
cd ..

echo ""
echo "=== Build complete ==="
echo "  npx-cli/gads-${VERSION}.tgz"
echo ""
echo "Install locally:"
echo "  npm install -g ./crates/gads/npx-cli/gads-${VERSION}.tgz"
