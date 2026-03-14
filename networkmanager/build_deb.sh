#!/usr/bin/env bash
set -euo pipefail

# Build a .deb package for the DrayTek VPN NetworkManager plugin.
# Includes: NM service, editor .so files, auth-dialog, tray binary,
# dispatcher script, D-Bus config, and NM service name file.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

VERSION="0.1.0"
ARCH="amd64"
PKG_NAME="draytek-vpn-nm"
PKG_DIR="$PROJECT_DIR/target/deb-nm/${PKG_NAME}_${VERSION}_${ARCH}"

# ── Build everything ──────────────────────────────────────────────
echo "Building Rust binaries (release)..."
cargo build -p draytek-vpn-nm -p draytek-vpn-tray --release

echo "Building editor plugin (.so)..."
make -C networkmanager/editor

echo "Building auth-dialog..."
make -C networkmanager/auth-dialog

# ── Create package structure ──────────────────────────────────────
rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/DEBIAN"
mkdir -p "$PKG_DIR/usr/lib/NetworkManager"
mkdir -p "$PKG_DIR/usr/lib/NetworkManager/VPN"
mkdir -p "$PKG_DIR/usr/lib/x86_64-linux-gnu/NetworkManager"
mkdir -p "$PKG_DIR/usr/libexec"
mkdir -p "$PKG_DIR/usr/bin"
mkdir -p "$PKG_DIR/etc/dbus-1/system.d"
mkdir -p "$PKG_DIR/etc/NetworkManager/dispatcher.d"

# ── Copy files ────────────────────────────────────────────────────

# NM service binary
install -m 755 target/release/draytek-vpn-nm \
    "$PKG_DIR/usr/lib/NetworkManager/nm-draytek-service"

# Editor plugins
install -m 755 networkmanager/editor/libnm-vpn-plugin-draytek.so \
    "$PKG_DIR/usr/lib/x86_64-linux-gnu/NetworkManager/"
install -m 755 networkmanager/editor/libnm-vpn-plugin-draytek-editor.so \
    "$PKG_DIR/usr/lib/x86_64-linux-gnu/NetworkManager/"
install -m 755 networkmanager/editor/libnm-gtk4-vpn-plugin-draytek-editor.so \
    "$PKG_DIR/usr/lib/x86_64-linux-gnu/NetworkManager/"

# Auth dialog
install -m 755 networkmanager/auth-dialog/nm-draytek-auth-dialog \
    "$PKG_DIR/usr/libexec/"

# Tray binary
install -m 755 target/release/draytek-vpn-tray \
    "$PKG_DIR/usr/bin/"

# Config files
install -m 644 networkmanager/data/nm-draytek-service.name \
    "$PKG_DIR/usr/lib/NetworkManager/VPN/"
install -m 644 networkmanager/data/nm-draytek-service.conf \
    "$PKG_DIR/etc/dbus-1/system.d/"
install -m 755 networkmanager/data/90-draytek-vpn-tray \
    "$PKG_DIR/etc/NetworkManager/dispatcher.d/"

# ── Calculate installed size ──────────────────────────────────────
INSTALLED_SIZE=$(du -sk "$PKG_DIR" | awk '{print $1}')

# ── Write control file ────────────────────────────────────────────
cat > "$PKG_DIR/DEBIAN/control" << EOF
Package: $PKG_NAME
Version: $VERSION
Architecture: $ARCH
Maintainer: Julian
Section: net
Priority: optional
Installed-Size: $INSTALLED_SIZE
Depends: network-manager, libssl3, libnm0, libgtk-4-1, libgtk-3-0
Description: DrayTek SSL VPN NetworkManager plugin
 Integrates DrayTek SSL VPN into NetworkManager so VPN connections
 appear in GNOME Settings, KDE, Cinnamon, or any NM frontend.
 Includes a system tray indicator that auto-launches on VPN connect.
EOF

# ── Write post-install script ─────────────────────────────────────
cat > "$PKG_DIR/DEBIAN/postinst" << 'EOF'
#!/bin/bash
set -e
# Restart NetworkManager to pick up the new plugin
if systemctl is-active --quiet NetworkManager; then
    systemctl restart NetworkManager
fi
EOF
chmod 755 "$PKG_DIR/DEBIAN/postinst"

# ── Write pre-remove script ──────────────────────────────────────
cat > "$PKG_DIR/DEBIAN/prerm" << 'EOF'
#!/bin/bash
set -e
# Kill any running tray instances
pkill -f draytek-vpn-tray 2>/dev/null || true
EOF
chmod 755 "$PKG_DIR/DEBIAN/prerm"

# ── Write post-remove script ─────────────────────────────────────
cat > "$PKG_DIR/DEBIAN/postrm" << 'EOF'
#!/bin/bash
set -e
# Restart NetworkManager to unload the plugin
if systemctl is-active --quiet NetworkManager; then
    systemctl restart NetworkManager
fi
EOF
chmod 755 "$PKG_DIR/DEBIAN/postrm"

# ── Build the .deb ────────────────────────────────────────────────
echo "Building .deb package..."
dpkg-deb --build --root-owner-group "$PKG_DIR"

DEB_FILE="$PROJECT_DIR/target/deb-nm/${PKG_NAME}_${VERSION}_${ARCH}.deb"
echo ""
echo "Package built successfully:"
ls -lh "$DEB_FILE"
echo ""
echo "Install with: sudo dpkg -i $DEB_FILE"
