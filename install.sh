#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;36m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Messages
EN_MESSAGES=(
    "Starting installation..."
    "Detected OS:"
    "Downloading latest release..."
    "URL:"
    "Installing binary..."
    "Cleaning up..."
    "Installation completed successfully!"
    "You can now use '%s' from your terminal"
    "Failed to download binary from:"
    "Failed to download the binary"
    "curl is required but not installed. Please install curl first."
    "sudo is required but not installed. Please install sudo first."
    "tar is required but not installed. Please install tar first."
    "Unsupported operating system"
    "Unsupported architecture:"
    "This script requires root privileges. Requesting sudo access..."
)

CN_MESSAGES=(
    "开始安装..."
    "检测到操作系统："
    "正在下载最新版本..."
    "下载地址："
    "正在安装程序..."
    "正在清理..."
    "安装成功完成！"
    "现在可以在终端中使用 '%s' 了"
    "从以下地址下载二进制文件失败："
    "下载二进制文件失败"
    "需要 curl 但未安装。请先安装 curl。"
    "需要 sudo 但未安装。请先安装 sudo。"
    "需要 tar 但未安装。请先安装 tar。"
    "不支持的操作系统"
    "不支持的架构："
    "此脚本需要root权限。正在请求sudo访问..."
)

# Detect system language
detect_language() {
    if [[ $(locale | grep "LANG=zh_CN") ]]; then
        echo "cn"
    else
        echo "en"
    fi
}

# Get message based on language
get_message() {
    local index=$1
    local lang=$(detect_language)
    
    if [[ "$lang" == "cn" ]]; then
        echo "${CN_MESSAGES[$index]}"
    else
        echo "${EN_MESSAGES[$index]}"
    fi
}

# Print with color
print_status() {
    echo -e "${BLUE}[*]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[✓]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[!]${NC} $1"
}

print_error() {
    echo -e "${RED}[✗]${NC} $1"
    exit 1
}

# Check and request root privileges
check_root() {
    if [ "$EUID" -ne 0 ]; then
        print_status "$(get_message 15)"
        if command -v sudo >/dev/null 2>&1; then
            exec sudo bash "$0" "$@"
        else
            print_error "$(get_message 11)"
        fi
    fi
}

# Detect OS
detect_os() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        echo "macos"
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        echo "linux"
    else
        print_error "$(get_message 13)"
    fi
}

# Get latest release version from GitHub
get_latest_version() {
    local repo="yunis-du/flash-cat"
    curl -s "https://api.github.com/repos/${repo}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
}

# Add download progress display function
download_with_progress() {
    local url="$1"
    local output_file="$2"
    
    curl -L -f --progress-bar "$url" -o "$output_file"
    return $?
}

# Optimize installation function
install_binary() {
    OS=$(detect_os)
    VERSION=$(get_latest_version)
    VERSION_WITHOUT_V=${VERSION#v}  # Remove 'v' from version number
    BINARY_NAME="flash-cat-cli-${OS}-${VERSION_WITHOUT_V}-$(get_arch).tar.gz"
    REPO="yunis-du/flash-cat"
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY_NAME}"
    TMP_DIR=$(mktemp -d)
    FINAL_BINARY_NAME="flash-cat"
    
    print_status "$(get_message 2)"
    print_status "$(get_message 3) ${DOWNLOAD_URL}"
    
    if ! download_with_progress "$DOWNLOAD_URL" "$TMP_DIR/$BINARY_NAME"; then
        rm -rf "$TMP_DIR"
        print_error "$(get_message 8) $DOWNLOAD_URL"
    fi
    
    if [ ! -f "$TMP_DIR/$BINARY_NAME" ]; then
        rm -rf "$TMP_DIR"
        print_error "$(get_message 9)"
    fi

    tar -xzf "$TMP_DIR/$BINARY_NAME" -C "$TMP_DIR"
    
    print_status "$(get_message 4)"
    INSTALL_DIR="/usr/local/bin"
    
    # Create directory if it doesn't exist
    mkdir -p "$INSTALL_DIR"
    
    # Move binary to installation directory
    if ! mv "$TMP_DIR/$FINAL_BINARY_NAME" "$INSTALL_DIR/$FINAL_BINARY_NAME"; then
        rm -rf "$TMP_DIR"
        print_error "Failed to move binary to installation directory"
    fi
    
    if ! chmod +x "$INSTALL_DIR/$FINAL_BINARY_NAME"; then
        rm -rf "$TMP_DIR"
        print_error "Failed to set executable permissions"
    fi
    
    # Cleanup
    print_status "$(get_message 5)"
    rm -rf "$TMP_DIR"
    
    print_success "$(get_message 6)"
    printf "${GREEN}[✓]${NC} $(get_message 7)\n" "$FINAL_BINARY_NAME"
    
    # Try to run the program directly
    if [ -x "$INSTALL_DIR/$FINAL_BINARY_NAME" ]; then
        "$INSTALL_DIR/$FINAL_BINARY_NAME" -v &
    else
        print_warning "Failed to start flash-cat"
    fi
}

# Optimize architecture detection function
get_arch() {
    case "$(uname -m)" in
        x86_64)
            echo "x86_64"
            ;;
        aarch64|arm64)
            echo "aarch64"
            ;;
        *)
            print_error "$(get_message 14) $(uname -m)"
            ;;
    esac
}

# Check for required tools
check_requirements() {
    if ! command -v curl >/dev/null 2>&1; then
        print_error "$(get_message 10)"
    fi
    
    if ! command -v sudo >/dev/null 2>&1; then
        print_error "$(get_message 11)"
    fi

    if ! command -v tar >/dev/null 2>&1; then
        print_error "$(get_message 12)"
    fi
}

# Main installation process
main() {
    print_status "$(get_message 0)"
    
    # Check root privileges
    check_root "$@"
    
    # Check required tools
    check_requirements
    
    OS=$(detect_os)
    print_status "$(get_message 1) $OS"
    
    # Install the binary
    install_binary
}

# Run main function
main "$@"