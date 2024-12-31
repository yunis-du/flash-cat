#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;36m'
YELLOW='\033[0;33m'
NC='\033[0m'

# Temporary directory for downloads
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

# Check for required commands
check_requirements() {
    if ! command -v curl >/dev/null 2>&1; then
        echo -e "${RED}Error: curl is required${NC}"
        exit 1
    fi

    if ! command -v tar >/dev/null 2>&1; then
        echo -e "${RED}Error: tar is required${NC}"
        exit 1
    fi
}

# Detect system information
detect_system() {
    local os arch suffix

    case "$(uname -s)" in
        Linux*)  os="linux";;
        Darwin*) os="macos";;
        *)       echo -e "${RED}Unsupported OS${NC}"; exit 1;;
    esac

    case "$(uname -m)" in
        x86_64)  
            arch="x86_64"
            ;;
        aarch64|arm64) 
            arch="aarch64"
            ;;
        *)       echo -e "${RED}Unsupported architecture${NC}"; exit 1;;
    esac

    echo "$os $arch"
}

# Download with progress
download() {
    local url="$1"
    local output="$2"
    curl -#L "$url" -o "$output"
}

# Check and create installation directory
setup_install_dir() {
    local install_dir="$1"
    
    if [ ! -d "$install_dir" ]; then
        mkdir -p "$install_dir" || {
            echo -e "${RED}Failed to create installation directory${NC}"
            exit 1
        }
    fi
}

# Main installation function
main() {
    check_requirements
    
    echo -e "${BLUE}Starting installation...${NC}"
    
    # Detect system
    read -r OS ARCH SUFFIX <<< "$(detect_system)"
    echo -e "${GREEN}Detected: $OS $ARCH${NC}"
    
    # Set installation directory
    INSTALL_DIR="/usr/local/bin"
    
    # Setup installation directory
    setup_install_dir "$INSTALL_DIR"
    
    # Get latest release info
    echo -e "${BLUE}Fetching latest release information...${NC}"
    LATEST_URL="https://api.github.com/repos/yunis-du/flash-cat/releases/latest"
    
    # Get latest version and remove 'v' prefix
    VERSION=$(curl -s "$LATEST_URL" | grep "tag_name" | cut -d'"' -f4 | sed 's/^v//')
    
    # Construct binary name
    BINARY_NAME="flash-cat-cli-${OS}-${VERSION}-${ARCH}"
    echo -e "${BLUE}Looking for asset: $BINARY_NAME${NC}"
    
    # Get download URL directly
    DOWNLOAD_URL=$(curl -s "$LATEST_URL" | grep -o "\"browser_download_url\": \"[^\"]*${BINARY_NAME}[^\"]*\"" | cut -d'"' -f4)
    
    if [ -z "$DOWNLOAD_URL" ]; then
        echo -e "${RED}Error: Could not find appropriate binary for $OS $ARCH${NC}"
        echo -e "${YELLOW}Available assets:${NC}"
        curl -s "$LATEST_URL" | grep "browser_download_url" | cut -d'"' -f4
        exit 1
    fi
    
    echo -e "${GREEN}Found matching asset: $BINARY_NAME${NC}"
    echo -e "${BLUE}Downloading from: $DOWNLOAD_URL${NC}"
    
    download "$DOWNLOAD_URL" "$TMP_DIR/$BINARY_NAME"

    tar -xvf "$TMP_DIR/$BINARY_NAME" -C "$TMP_DIR"
    
    # Install binary
    echo -e "${BLUE}Installing...${NC}"
    chmod +x "$TMP_DIR/flash-cat"
    sudo mv "$TMP_DIR/flash-cat" "$INSTALL_DIR/"
    
    echo -e "${GREEN}Installation completed successfully!${NC}"
    echo -e "${BLUE}Show flash-cat version...${NC}"
    
    # Run the program with sudo, preserving environment variables
    export AUTOMATED_MODE=1
    if ! flash-cat -v; then
        echo -e "${RED}Failed to run flash-cat${NC}"
        exit 1
    fi
}

main
