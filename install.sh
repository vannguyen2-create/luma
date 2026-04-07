#!/bin/sh
# Install or update Luma — lightweight coding agent
# Usage: curl -fsSL https://raw.githubusercontent.com/nghyane/luma/master/install.sh | sh
set -e

REPO="nghyane/luma"
INSTALL_DIR="${LUMA_INSTALL_DIR:-$HOME/.local/bin}"

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin) os="apple-darwin" ;;
  Linux)  os="unknown-linux-gnu" ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
esac

TARGET="${arch}-${os}"

# Find latest release (or use LUMA_VERSION env)
if [ -n "$LUMA_VERSION" ]; then
  TAG="$LUMA_VERSION"
else
  TAG=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep '"tag_name"' | head -1 | cut -d'"' -f4)
fi

if [ -z "$TAG" ]; then
  echo "Failed to detect latest version" >&2
  exit 1
fi

URL="https://github.com/$REPO/releases/download/$TAG/luma-${TARGET}.tar.gz"

echo "Installing luma $TAG ($TARGET)"
echo "  from: $URL"
echo "  to:   $INSTALL_DIR/luma"

# Download and extract
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

curl -fsSL "$URL" -o "$TMP/luma.tar.gz"
tar xzf "$TMP/luma.tar.gz" -C "$TMP"

# Install
mkdir -p "$INSTALL_DIR"
mv "$TMP/luma" "$INSTALL_DIR/luma"
chmod +x "$INSTALL_DIR/luma"

echo "Installed luma $TAG"

# Check PATH
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo ""
    echo "Add to your shell profile:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac
