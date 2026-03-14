#!/usr/bin/env bash
set -euo pipefail

# Install build dependencies for DrayTek VPN components.
#
# Usage:
#   ./install_dependencies.sh app       Standalone GTK4 app
#   ./install_dependencies.sh nm        NetworkManager plugin
#   ./install_dependencies.sh all       Everything

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

install_deps() {
    local dep_file="$1"
    if [[ ! -f "$dep_file" ]]; then
        echo "Error: $dep_file not found"
        exit 1
    fi

    mapfile -t PACKAGES < <(grep -v '^\s*#' "$dep_file" | grep -v '^\s*$')

    if command -v apt &>/dev/null; then
        echo "Detected Debian/Ubuntu — using apt"
        echo "Installing: ${PACKAGES[*]}"
        sudo apt install -y "${PACKAGES[@]}"
    elif command -v dnf &>/dev/null; then
        echo "Detected Fedora/RHEL — using dnf"
        # Map Debian package names to Fedora equivalents
        declare -A FEDORA_MAP=(
            [build-essential]="gcc gcc-c++ make"
            [pkg-config]="pkgconf-pkg-config"
            [libgtk-4-dev]="gtk4-devel"
            [libgtk-3-dev]="gtk3-devel"
            [libadwaita-1-dev]="libadwaita-devel"
            [libssl-dev]="openssl-devel"
            [libnm-dev]="NetworkManager-libnm-devel"
        )
        FEDORA_PACKAGES=()
        for pkg in "${PACKAGES[@]}"; do
            if [[ -n "${FEDORA_MAP[$pkg]+x}" ]]; then
                # shellcheck disable=SC2206
                FEDORA_PACKAGES+=(${FEDORA_MAP[$pkg]})
            else
                FEDORA_PACKAGES+=("$pkg")
            fi
        done
        echo "Installing: ${FEDORA_PACKAGES[*]}"
        sudo dnf install -y "${FEDORA_PACKAGES[@]}"
    else
        echo "Error: Unsupported package manager. Install these packages manually:"
        printf '  %s\n' "${PACKAGES[@]}"
        exit 1
    fi
}

target="${1:-}"
case "$target" in
    app)
        echo "Installing standalone app dependencies..."
        install_deps "$SCRIPT_DIR/standalone/dependencies.txt"
        ;;
    nm)
        echo "Installing NetworkManager plugin dependencies..."
        install_deps "$SCRIPT_DIR/networkmanager/dependencies.txt"
        ;;
    all)
        echo "Installing all dependencies..."
        install_deps "$SCRIPT_DIR/networkmanager/dependencies.txt"
        ;;
    *)
        echo "Usage: ./install_dependencies.sh <app|nm|all>"
        exit 1
        ;;
esac

echo "Done — all dependencies installed."
