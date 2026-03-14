#!/usr/bin/env bash
set -euo pipefail

# Build an AppImage for the DrayTek VPN standalone app.
# The helper binary and polkit policy are bundled inside the AppImage
# but require manual installation for privilege separation to work.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

TOOLS_DIR="$SCRIPT_DIR/appimage-tools"
LINUXDEPLOY="$TOOLS_DIR/linuxdeploy-x86_64.AppImage"
GTK_PLUGIN="$TOOLS_DIR/linuxdeploy-plugin-gtk.sh"

# ── Download tools ────────────────────────────────────────────────
mkdir -p "$TOOLS_DIR"

if [[ ! -x "$LINUXDEPLOY" ]]; then
    echo "Downloading linuxdeploy..."
    wget -O "$LINUXDEPLOY" \
        "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
    chmod +x "$LINUXDEPLOY"
fi

if [[ ! -x "$GTK_PLUGIN" ]]; then
    echo "Downloading linuxdeploy-plugin-gtk..."
    wget -O "$GTK_PLUGIN" \
        "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh"
    chmod +x "$GTK_PLUGIN"
fi

# ── Build release binaries ────────────────────────────────────────
echo "Building release binaries..."
cargo build -p draytek-vpn --release

# ── Set up AppDir ─────────────────────────────────────────────────
rm -rf "$SCRIPT_DIR/AppDir"
mkdir -p "$SCRIPT_DIR/AppDir/usr/bin"
mkdir -p "$SCRIPT_DIR/AppDir/usr/lib/draytek-vpn"
mkdir -p "$SCRIPT_DIR/AppDir/usr/share/polkit-1/actions"

cp "$PROJECT_DIR/target/release/draytek-vpn" "$SCRIPT_DIR/AppDir/usr/bin/"
cp "$PROJECT_DIR/target/release/draytek-vpn-helper" "$SCRIPT_DIR/AppDir/usr/lib/draytek-vpn/"
cp "$SCRIPT_DIR/data/com.draytek.vpn.policy" "$SCRIPT_DIR/AppDir/usr/share/polkit-1/actions/"

# ── Find or create an icon ─────────────────────────────────────────
ICON_TMP="$(mktemp -d)/draytek-vpn.png"
# Try common icon locations, fall back to generating a simple one
if [[ -f "/usr/share/icons/Mint-Y/devices/128/network-vpn.png" ]]; then
    cp /usr/share/icons/Mint-Y/devices/128/network-vpn.png "$ICON_TMP"
elif FOUND_ICON=$(find /usr/share/icons -name "network-vpn.png" -size +1k 2>/dev/null | head -1) && [[ -n "$FOUND_ICON" ]]; then
    cp "$FOUND_ICON" "$ICON_TMP"
elif FOUND_SVG=$(find /usr/share/icons -name "network-vpn.svg" 2>/dev/null | head -1) && [[ -n "$FOUND_SVG" ]]; then
    if command -v rsvg-convert &>/dev/null; then
        rsvg-convert -w 256 -h 256 "$FOUND_SVG" -o "$ICON_TMP"
    else
        # linuxdeploy can handle SVG
        ICON_TMP="${ICON_TMP%.png}.svg"
        cp "$FOUND_SVG" "$ICON_TMP"
    fi
else
    error "No network-vpn icon found. Install an icon theme or place an icon at standalone/data/draytek-vpn.png"
    exit 1
fi

# ── Build AppImage ────────────────────────────────────────────────
export DEPLOY_GTK_VERSION=4
export PATH="$TOOLS_DIR:$PATH"

"$LINUXDEPLOY" \
    --appdir "$SCRIPT_DIR/AppDir" \
    --executable "$PROJECT_DIR/target/release/draytek-vpn" \
    --desktop-file "$SCRIPT_DIR/data/draytek-vpn.desktop" \
    --icon-file "$ICON_TMP" \
    --plugin gtk \
    --output appimage

echo ""
echo "AppImage built successfully:"
ls -lh DrayTek_VPN*.AppImage 2>/dev/null || ls -lh draytek*.AppImage 2>/dev/null || ls -lh *.AppImage
echo ""
echo "NOTE: The helper binary and polkit policy are bundled but need"
echo "manual installation for privilege separation to work:"
echo "  sudo install -m 755 AppDir/usr/lib/draytek-vpn/draytek-vpn-helper /usr/lib/draytek-vpn/"
echo "  sudo install -m 644 AppDir/usr/share/polkit-1/actions/com.draytek.vpn.policy /usr/share/polkit-1/actions/"
