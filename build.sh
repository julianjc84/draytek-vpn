#!/bin/bash
# Build & install script for DrayTek SSL VPN Client.
#
# Targets:
#   app  — Standalone GTK4 app (draytek-vpn + draytek-vpn-helper)
#   nm   — NetworkManager plugin (service, editor .so, auth-dialog)
#   tray — System tray indicator (draytek-vpn-tray)
#   all  — All of the above
#
# Usage:
#   ./build.sh app                    Build standalone app (debug)
#   ./build.sh app release            Build standalone app (release)
#   ./build.sh app install            Build release + install polkit policy
#   ./build.sh app run                Build debug + launch the app
#   ./build.sh nm                     Build NM plugin (debug)
#   ./build.sh nm release             Build NM plugin (release)
#   ./build.sh nm install             Build release + install + restart NM
#   ./build.sh nm uninstall           Remove NM plugin files + restart NM
#   ./build.sh tray                   Build tray indicator (debug)
#   ./build.sh tray release           Build tray indicator (release)
#   ./build.sh tray install           Build release + install binary + autostart
#   ./build.sh tray uninstall         Remove tray binary + autostart
#   ./build.sh all                    Build everything (debug)
#   ./build.sh all release            Build everything (release)
#   ./build.sh all install            Build + install everything
#   ./build.sh clean                  Remove all build artifacts

set -euo pipefail

cd "$(dirname "$0")"

# ── Install paths ──────────────────────────────────────────────────
POLKIT_DIR="/usr/share/polkit-1/actions"
NM_PLUGIN_DIR="/usr/lib/x86_64-linux-gnu/NetworkManager"
NM_VPN_DIR="/usr/lib/NetworkManager/VPN"
NM_SERVICE_DIR="/usr/lib/NetworkManager"
DBUS_CONF_DIR="/etc/dbus-1/system.d"
LIBEXEC_DIR="/usr/libexec"
NM_DISPATCHER_DIR="/etc/NetworkManager/dispatcher.d"

# ── Colors ─────────────────────────────────────────────────────────
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${GREEN}[+]${NC} $*"; }
warn()  { echo -e "${YELLOW}[!]${NC} $*"; }
error() { echo -e "${RED}[-]${NC} $*" >&2; }
header(){ echo -e "\n${BOLD}── $* ──${NC}"; }

# ── App ────────────────────────────────────────────────────────────

app_build() {
    local profile="${1:-debug}"
    local flags=""
    [ "$profile" = "release" ] && flags="--release"

    header "Standalone App ($profile)"
    info "Building draytek-vpn + draytek-vpn-helper"
    cargo build -p draytek-vpn $flags

    info "Artifacts:"
    echo "  target/$profile/draytek-vpn"
    echo "  target/$profile/draytek-vpn-helper"
}

app_install() {
    app_build release

    header "Install App"
    info "Installing polkit policy (requires sudo)"
    sudo install -m 644 standalone/data/com.draytek.vpn.policy "$POLKIT_DIR/"
    info "Done. Run with: cargo run --bin draytek-vpn"
}

app_run() {
    app_build debug
    header "Launching"
    cargo run --bin draytek-vpn
}

# ── NM Plugin ──────────────────────────────────────────────────────

nm_build() {
    local profile="${1:-debug}"
    local flags=""
    [ "$profile" = "release" ] && flags="--release"

    header "NetworkManager Plugin ($profile)"
    info "Building Rust NM service"
    cargo build -p draytek-vpn-nm $flags

    info "Building editor plugin (.so)"
    make -C networkmanager/editor

    info "Building auth-dialog"
    make -C networkmanager/auth-dialog

    info "Artifacts:"
    echo "  target/$profile/draytek-vpn-nm"
    echo "  networkmanager/editor/libnm-vpn-plugin-draytek.so"
    echo "  networkmanager/editor/libnm-gtk4-vpn-plugin-draytek-editor.so"
    echo "  networkmanager/auth-dialog/nm-draytek-auth-dialog"
}

nm_install() {
    nm_build release

    header "Install NM Plugin"
    info "Installing files (requires sudo)"

    sudo install -m 755 target/release/draytek-vpn-nm \
        "$NM_SERVICE_DIR/nm-draytek-service"
    sudo install -m 755 networkmanager/editor/libnm-vpn-plugin-draytek.so \
        "$NM_PLUGIN_DIR/"
    sudo install -m 755 networkmanager/editor/libnm-vpn-plugin-draytek-editor.so \
        "$NM_PLUGIN_DIR/"
    sudo install -m 755 networkmanager/editor/libnm-gtk4-vpn-plugin-draytek-editor.so \
        "$NM_PLUGIN_DIR/"
    sudo install -m 755 networkmanager/auth-dialog/nm-draytek-auth-dialog \
        "$LIBEXEC_DIR/"
    sudo install -m 644 networkmanager/data/nm-draytek-service.name \
        "$NM_VPN_DIR/"
    sudo install -m 644 networkmanager/data/nm-draytek-service.conf \
        "$DBUS_CONF_DIR/"
    sudo install -m 755 networkmanager/data/90-draytek-vpn-tray \
        "$NM_DISPATCHER_DIR/"

    info "Restarting NetworkManager..."
    sudo systemctl restart NetworkManager
    info "Installed. DrayTek SSL VPN should appear in GNOME Settings > VPN."
    info "Tray indicator will auto-launch on VPN connect (requires draytek-vpn-tray in /usr/bin)."
}

nm_uninstall() {
    header "Uninstall NM Plugin"
    info "Removing files (requires sudo)"

    sudo rm -f "$NM_SERVICE_DIR/nm-draytek-service"
    sudo rm -f "$NM_PLUGIN_DIR/libnm-vpn-plugin-draytek.so"
    sudo rm -f "$NM_PLUGIN_DIR/libnm-vpn-plugin-draytek-editor.so"
    sudo rm -f "$NM_PLUGIN_DIR/libnm-gtk4-vpn-plugin-draytek-editor.so"
    sudo rm -f "$LIBEXEC_DIR/nm-draytek-auth-dialog"
    sudo rm -f "$NM_VPN_DIR/nm-draytek-service.name"
    sudo rm -f "$DBUS_CONF_DIR/nm-draytek-service.conf"
    sudo rm -f "$NM_DISPATCHER_DIR/90-draytek-vpn-tray"

    info "Restarting NetworkManager..."
    sudo systemctl restart NetworkManager
    info "Uninstalled."
}

# ── Tray Indicator ────────────────────────────────────────────────

tray_build() {
    local profile="${1:-debug}"
    local flags=""
    [ "$profile" = "release" ] && flags="--release"

    header "Tray Indicator ($profile)"
    info "Building draytek-vpn-tray"
    cargo build -p draytek-vpn-tray $flags

    info "Artifacts:"
    echo "  target/$profile/draytek-vpn-tray"
}

tray_install() {
    tray_build release

    header "Install Tray Indicator"
    info "Installing binary (requires sudo)"
    sudo install -m 755 target/release/draytek-vpn-tray /usr/bin/

    info "Installed. Tray launches automatically on VPN connect (via NM dispatcher)."
}

tray_uninstall() {
    header "Uninstall Tray Indicator"
    info "Removing files"
    sudo rm -f /usr/bin/draytek-vpn-tray
    info "Uninstalled."
}

# ── Clean ──────────────────────────────────────────────────────────

do_clean() {
    header "Clean"
    info "Cleaning C build artifacts"
    make -C networkmanager/editor clean
    make -C networkmanager/auth-dialog clean
    info "Done. Run 'cargo clean' separately for Rust targets."
}

# ── Usage ──────────────────────────────────────────────────────────

usage() {
    cat <<'EOF'
Usage: ./build.sh <target> [action]

Targets:
  app              Standalone GTK4 app
  nm               NetworkManager plugin
  tray             System tray indicator
  all              All of the above

Actions:
  (default)        Build debug
  release          Build release
  install          Build release + install system-wide
  run              Build debug + launch (app only)
  uninstall        Remove installed files (nm, tray)

  clean            Remove build artifacts (no target needed)

Examples:
  ./build.sh app                Build standalone app (debug)
  ./build.sh app run            Build + launch the app
  ./build.sh nm install         Build + install NM plugin
  ./build.sh tray install       Build + install tray indicator
  ./build.sh all release        Build everything (release)
  ./build.sh clean              Clean C artifacts
EOF
    exit 1
}

# ── Main ───────────────────────────────────────────────────────────

target="${1:-}"
action="${2:-debug}"

case "$target" in
    app)
        case "$action" in
            debug|"")   app_build debug ;;
            release)    app_build release ;;
            install)    app_install ;;
            run)        app_run ;;
            *)          usage ;;
        esac
        ;;
    nm)
        case "$action" in
            debug|"")   nm_build debug ;;
            release)    nm_build release ;;
            install)    nm_install ;;
            uninstall)  nm_uninstall ;;
            *)          usage ;;
        esac
        ;;
    tray)
        case "$action" in
            debug|"")   tray_build debug ;;
            release)    tray_build release ;;
            install)    tray_install ;;
            uninstall)  tray_uninstall ;;
            *)          usage ;;
        esac
        ;;
    all)
        case "$action" in
            debug|"")   app_build debug;  nm_build debug;  tray_build debug ;;
            release)    app_build release; nm_build release; tray_build release ;;
            install)    app_install; nm_install; tray_install ;;
            *)          usage ;;
        esac
        ;;
    clean)
        do_clean
        ;;
    *)
        usage
        ;;
esac
