#!/bin/bash
set -euo pipefail

REPO="kotmorakot/token-pipline"
INSTALL_DIR="${HOME}/.local/bin"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
    linux)  OS_NAME="linux" ;;
    darwin) OS_NAME="macos" ;;
    *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
    x86_64|amd64) ARCH_NAME="x86_64" ;;
    aarch64|arm64) ARCH_NAME="aarch64" ;;
    *)             echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

ARTIFACT="tp-${OS_NAME}-${ARCH_NAME}"
LATEST_URL="https://github.com/${REPO}/releases/latest/download/${ARTIFACT}.tar.gz"

echo "Installing tp (token-pipeline)..."
echo "  OS:   ${OS_NAME}"
echo "  Arch: ${ARCH_NAME}"
echo "  From: ${LATEST_URL}"
echo

mkdir -p "$INSTALL_DIR"
curl -fsSL "$LATEST_URL" | tar xz -C "$INSTALL_DIR"
chmod +x "${INSTALL_DIR}/tp" 2>/dev/null || chmod +x "${INSTALL_DIR}/${ARTIFACT}" 2>/dev/null

if [ -f "${INSTALL_DIR}/${ARTIFACT}" ] && [ ! -f "${INSTALL_DIR}/tp" ]; then
    mv "${INSTALL_DIR}/${ARTIFACT}" "${INSTALL_DIR}/tp"
fi

echo "Installed tp to ${INSTALL_DIR}/tp"
echo

if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
    echo "Add to your PATH:"
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    echo
fi

"${INSTALL_DIR}/tp" --version
echo
echo "Run 'tp init' to set up auto-hooks."
