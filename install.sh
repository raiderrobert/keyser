#!/bin/sh
# Keyser installer — https://github.com/raiderrobert/keyser
# Usage: curl -fsSL https://raw.githubusercontent.com/raiderrobert/keyser/main/install.sh | sh
set -e

REPO="raiderrobert/keyser"
INSTALL_DIR="${KEYSER_INSTALL_DIR:-/usr/local/bin}"

main() {
    platform="$(detect_platform)"
    arch="$(detect_arch)"
    asset="$(asset_name "$platform" "$arch")"

    if [ -z "$asset" ]; then
        echo "Error: unsupported platform/architecture: ${platform}/${arch}" >&2
        echo "Pre-built binaries are available for:" >&2
        echo "  - macOS (Apple Silicon / aarch64)" >&2
        echo "" >&2
        echo "You can build from source instead: cargo install --path ." >&2
        exit 1
    fi

    url="https://github.com/${REPO}/releases/latest/download/${asset}"

    echo "Detected: ${platform}/${arch}"
    echo "Downloading: ${url}"

    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    if command -v curl > /dev/null 2>&1; then
        curl -fsSL "$url" -o "${tmpdir}/${asset}"
    elif command -v wget > /dev/null 2>&1; then
        wget -qO "${tmpdir}/${asset}" "$url"
    else
        echo "Error: curl or wget is required" >&2
        exit 1
    fi

    tar xzf "${tmpdir}/${asset}" -C "$tmpdir"

    if [ -w "$INSTALL_DIR" ]; then
        mv "${tmpdir}/keyser" "${INSTALL_DIR}/keyser"
    else
        echo "Installing to ${INSTALL_DIR} (requires sudo)"
        sudo mv "${tmpdir}/keyser" "${INSTALL_DIR}/keyser"
    fi

    chmod +x "${INSTALL_DIR}/keyser"

    echo "Installed keyser to ${INSTALL_DIR}/keyser"
    "${INSTALL_DIR}/keyser" 2>/dev/null || true
}

detect_platform() {
    case "$(uname -s)" in
        Darwin*) echo "macos" ;;
        *)       echo "unknown" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        arm64|aarch64) echo "aarch64" ;;
        *)             echo "unknown" ;;
    esac
}

asset_name() {
    platform="$1"
    arch="$2"

    case "${arch}-${platform}" in
        aarch64-macos) echo "keyser-aarch64-macos.tar.gz" ;;
        *)             echo "" ;;
    esac
}

main
