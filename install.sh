#!/bin/bash
# EVT3 CLI Installer
# Downloads and installs the evt3 command-line tool
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/muthmann/evt3/main/install.sh | bash
#
# Or with a specific version:
#   curl -sSL https://raw.githubusercontent.com/muthmann/evt3/main/install.sh | bash -s -- v0.1.0

set -e

REPO="muthmann/evt3"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Detect OS and architecture
detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$OS" in
        linux) OS="linux" ;;
        darwin) OS="macos" ;;
        mingw*|msys*|cygwin*) OS="windows" ;;
        *) error "Unsupported OS: $OS" ;;
    esac

    case "$ARCH" in
        x86_64|amd64) ARCH="x64" ;;
        aarch64|arm64) ARCH="arm64" ;;
        *) error "Unsupported architecture: $ARCH" ;;
    esac

    PLATFORM="${OS}-${ARCH}"
    info "Detected platform: $PLATFORM"
}

# Get latest release version
get_version() {
    if [ -n "$1" ]; then
        VERSION="$1"
    else
        VERSION=$(curl -sSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
        if [ -z "$VERSION" ]; then
            error "Could not determine latest version"
        fi
    fi
    info "Installing version: $VERSION"
}

# Download and install
install() {
    BINARY_NAME="evt3-${PLATFORM}"
    if [ "$OS" = "windows" ]; then
        BINARY_NAME="${BINARY_NAME}.exe"
    fi

    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$BINARY_NAME"
    
    info "Downloading from: $DOWNLOAD_URL"
    
    # Create install directory
    mkdir -p "$INSTALL_DIR"
    
    # Download
    if command -v curl &> /dev/null; then
        curl -sSL "$DOWNLOAD_URL" -o "$INSTALL_DIR/evt3" || error "Download failed"
    elif command -v wget &> /dev/null; then
        wget -q "$DOWNLOAD_URL" -O "$INSTALL_DIR/evt3" || error "Download failed"
    else
        error "Neither curl nor wget found"
    fi
    
    # Make executable
    chmod +x "$INSTALL_DIR/evt3"
    
    info "Installed to: $INSTALL_DIR/evt3"
}

# Check PATH
check_path() {
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        warn "$INSTALL_DIR is not in your PATH"
        echo ""
        echo "Add it to your shell profile:"
        echo ""
        echo "  # For bash (~/.bashrc or ~/.bash_profile):"
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
        echo "  # For zsh (~/.zshrc):"
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
        echo "Then restart your shell or run: source ~/.bashrc"
    fi
}

# Verify installation
verify() {
    if [ -x "$INSTALL_DIR/evt3" ]; then
        echo ""
        info "Installation successful!"
        echo ""
        "$INSTALL_DIR/evt3" --version
        echo ""
        echo "Usage:"
        echo "  evt3 recording.raw output.csv"
        echo ""
        check_path
    else
        error "Installation failed"
    fi
}

# Main
main() {
    echo "╔═══════════════════════════════════════╗"
    echo "║       EVT3 Decoder Installer          ║"
    echo "╚═══════════════════════════════════════╝"
    echo ""
    
    detect_platform
    get_version "$1"
    install
    verify
}

main "$@"
