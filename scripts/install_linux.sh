#!/usr/bin/env bash
set -euo pipefail

C_RESET="\033[0m"
C_BOLD="\033[1m"
C_GREEN="\033[32m"
C_CYAN="\033[36m"
C_YELLOW="\033[33m"
C_RED="\033[31m"

TOTAL_STEPS=6

step() {
    local num="$1"
    local title="$2"
    echo -e "\n${C_BOLD}${C_CYAN}[${num}/${TOTAL_STEPS}]${C_RESET} ${C_BOLD}${title}${C_RESET}"
}

success() {
    echo -e "  ${C_GREEN}✓${C_RESET} $1"
}

info() {
    echo -e "  ${C_YELLOW}→${C_RESET} $1"
}

error_exit() {
    echo -e "\n${C_RED}${C_BOLD}[ERROR]${C_RESET} $1" >&2
    exit 1
}

# Resolve repository root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

echo -e "${C_BOLD}${C_GREEN}"
cat << 'EOF'
 ▄░░░▄ ░░░░▄ ▄░░░ ░░ ▄░░▄░░░▄ ░░ ░░ ░░░▄      ░░ ░░░░▄ ▄░░░ ░░░░░░  ▄░░░▄ ░░  ░░  ▄░░░  ░░░░▄
 ░░ ░░ ░░ ░░ ░░   ░░ ░░ ░░ ░░ ░░ ░░   ░░      ░░ ░░ ░░ ░░   ▀ ░░ ▀  ░░ ░░ ░░  ░░  ░░ ▀  ░░ ░░
 ▒▒▒▒▒ ▒▒ ▒▒ ▀▒▒▄ ▒▒ ▒▒ ▒▒ ▒▒ ▒▒ ▒▒ ▄▒▒▀      ▒▒ ▒▒ ▒▒ ▀▒▒▄   ▒▒    ▒▒▒▒▒ ▒▒  ▒▒  ▒▒▒   ▒▒▒▒▀
 ▓█ ▓█ ▓█ ▓█   ▓█ ▓█ ▓█ ▓█ ▓█ ▓█ ▓█ ▓█        ▓█ ▓█ ▓█   ▓█   ▓█    ▓█ ▓█ ▓█  ▓█  ▓█    ▓█ ▓█
 ▄▄ ▄▄ ▄▄ ▄▄   ▄▄ ▄▄ ▄▄ ▄▄ ▄▄ ▄▄ ▄▄ ▄▄        ▄▄ ▄▄ ▄▄   ▄▄   ▄▄    ▄▄ ▄▄ ▄▄  ▄▄  ▄▄    ▄▄ ▄▄
 ░░ ░░ ░░ ░░   ░░ ░░ ░░ ░░ ░░ ░░ ░░ ░░        ░░ ░░ ░░   ░░   ░░    ░░ ░░ ▓▓  ▓▓  ░░    ░░ ░░
 ▒▒ ▒▒ ▒▒ ▒▒   ▒▒ ▒▒ ▒▒    ▒▒ ▒▒ ▒▒ ▒▒        ▒▒ ▒▒ ▒▒   ▒▒   ▒▒    ▒▒ ▒▒ ▒▒  ▒▒  ▒▒ ▄  ▒▒ ▒▒
▄▓▓ ▓▓ ▓▓ ▓▓ ▓▓▓▀ ▓▓ ▓▓    ▓▓ ▀▓▓▓▀ ▀▓▓▓      ▓▓ ▓▓ ▓▓ ▓▓▓▀  ▄▓▓▄  ▄▓▓ ▓▓ ▀▓▓ ▀▓▓ ▀▓▓▓ ▄▓▓ ▓▓
EOF
echo -e "${C_RESET}"
echo -e "${C_BOLD}ANSImuz Linux Automated Installation Script${C_RESET}"
echo -e "Target repository: ${REPO_ROOT}\n"

# -------------------------------------------------------------
# Detect Linux distribution & package manager
# -------------------------------------------------------------
step "1" "Detecting system package manager..."

PKG_MANAGER=""
INSTALL_CMD=""

if command -v apt-get >/dev/null 2>&1; then
    PKG_MANAGER="apt"
    INSTALL_CMD="sudo apt-get update && sudo apt-get install -y"
elif command -v pacman >/dev/null 2>&1; then
    PKG_MANAGER="pacman"
    INSTALL_CMD="sudo pacman -S --needed --noconfirm"
elif command -v dnf >/dev/null 2>&1; then
    PKG_MANAGER="dnf"
    INSTALL_CMD="sudo dnf install -y"
elif command -v zypper >/dev/null 2>&1; then
    PKG_MANAGER="zypper"
    INSTALL_CMD="sudo zypper install -y"
else
    info "No standard package manager detected (apt, pacman, dnf, zypper)."
fi

success "Detected package manager: ${PKG_MANAGER:-manual}"

# -------------------------------------------------------------
# Install required system packages (mpv, yt-dlp, dev libs)
# -------------------------------------------------------------
step "2" "Installing system prerequisites (mpv, yt-dlp, build dependencies)..."

MISSING_PACKAGES=()

check_or_queue() {
    local cmd="$1"
    local pkg_apt="$2"
    local pkg_pacman="$3"
    local pkg_dnf="$4"

    if ! command -v "$cmd" >/dev/null 2>&1; then
        case "$PKG_MANAGER" in
            apt) MISSING_PACKAGES+=("$pkg_apt") ;;
            pacman) MISSING_PACKAGES+=("$pkg_pacman") ;;
            dnf) MISSING_PACKAGES+=("$pkg_dnf") ;;
            zypper) MISSING_PACKAGES+=("$pkg_apt") ;;
        esac
    else
        success "${cmd} is already installed."
    fi
}

check_or_queue "mpv" "mpv" "mpv" "mpv"
check_or_queue "yt-dlp" "yt-dlp" "yt-dlp" "yt-dlp"
check_or_queue "pkg-config" "pkg-config" "pkgconf" "pkgconf-pkg-config"
check_or_queue "python3" "python3" "python" "python3"

check_dev_pkg() {
    local pkg="$1"
    case "$PKG_MANAGER" in
        apt) dpkg -s "$pkg" >/dev/null 2>&1 || MISSING_PACKAGES+=("$pkg") ;;
        pacman) pacman -Qi "$pkg" >/dev/null 2>&1 || MISSING_PACKAGES+=("$pkg") ;;
        dnf) rpm -q "$pkg" >/dev/null 2>&1 || MISSING_PACKAGES+=("$pkg") ;;
    esac
}

if [ "$PKG_MANAGER" = "apt" ]; then
    for p in python3-venv python3-pip python3-dev build-essential libssl-dev; do
        check_dev_pkg "$p"
    done
elif [ "$PKG_MANAGER" = "pacman" ]; then
    for p in python-pip base-devel openssl; do
        check_dev_pkg "$p"
    done
elif [ "$PKG_MANAGER" = "dnf" ]; then
    for p in python3-pip python3-devel gcc openssl-devel; do
        check_dev_pkg "$p"
    done
fi

if [ ${#MISSING_PACKAGES[@]} -gt 0 ]; then
    info "Installing required packages via ${PKG_MANAGER}: ${MISSING_PACKAGES[*]}"
    if [ -n "$INSTALL_CMD" ]; then
        eval "${INSTALL_CMD} ${MISSING_PACKAGES[*]}" || {
            info "Package manager installation encountered an issue. Continuing with checks..."
        }
    fi
fi

# Fallback check for yt-dlp if package manager did not provide it
if ! command -v yt-dlp >/dev/null 2>&1; then
    info "yt-dlp not found in PATH; attempting standalone binary install..."
    sudo curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/local/bin/yt-dlp
    sudo chmod a+rx /usr/local/bin/yt-dlp
fi

command -v mpv >/dev/null 2>&1 || error_exit "mpv is required for audio playback. Please install mpv manually."
command -v yt-dlp >/dev/null 2>&1 || error_exit "yt-dlp is required for media streaming. Please install yt-dlp manually."

success "System prerequisites verified."

# -------------------------------------------------------------
# Verify or install Rust toolchain
# -------------------------------------------------------------
step "3" "Checking Rust toolchain..."

if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
fi

if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
    info "Rust toolchain not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck source=/dev/null
    source "$HOME/.cargo/env"
fi

RUSTC_VER="$(rustc --version)"
CARGO_VER="$(cargo --version)"
success "Rust toolchain ready: ${RUSTC_VER}"

# -------------------------------------------------------------
# Setup Python virtual environment
# -------------------------------------------------------------
step "4" "Setting up Python virtual environment (.venv)..."

if [ ! -d ".venv" ]; then
    info "Creating virtual environment in ${REPO_ROOT}/.venv..."
    python3 -m venv .venv || error_exit "Failed to create Python virtual environment. Ensure python3-venv is installed."
fi

# Activate venv
# shellcheck source=/dev/null
source .venv/bin/activate
VENV_PYTHON="$(which python3)"
success "Virtual environment activated: ${VENV_PYTHON}"

# -------------------------------------------------------------
# Install Python requirements
# -------------------------------------------------------------
step "5" "Installing Python dependencies from requirements.txt..."

pip install --upgrade pip --quiet
pip install -r requirements.txt --quiet
success "Python packages successfully installed (ytmusicapi, spotifyscraper, pillow, requests)."

# -------------------------------------------------------------
# Compile ANSImuz with Cargo
# -------------------------------------------------------------
step "6" "Building ANSImuz release binary..."

cargo build --release

BIN_PATH="${REPO_ROOT}/target/release/ansimuz"
if [ ! -f "${BIN_PATH}" ]; then
    error_exit "Compiled binary was not found at ${BIN_PATH}."
fi

success "Binary compiled successfully: ${BIN_PATH}"

# -------------------------------------------------------------
# ALL DONE
# -------------------------------------------------------------
echo -e "\n${C_BOLD}${C_GREEN}======================================================${C_RESET}"
echo -e "${C_BOLD}${C_GREEN}  Installation Complete! ANSImuz is ready to run.     ${C_RESET}"
echo -e "${C_BOLD}${C_GREEN}======================================================${C_RESET}"
echo -e "\nTo launch ANSImuz:"
echo -e "  ${C_CYAN}source .venv/bin/activate${C_RESET}"
echo -e "  ${C_CYAN}cargo run --release${C_RESET}"
echo -e "or execute the binary directly:"
echo -e "  ${C_CYAN}./target/release/ansimuz${C_RESET}\n"
