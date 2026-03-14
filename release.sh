#!/bin/bash
# Build all packages and create a GitHub release.
#
# Usage:
#   ./release.sh              Build packages, tag, and upload to GitHub
#   ./release.sh --build-only Build packages without creating a release
#   ./release.sh --tag v0.2.0 Override the version tag (default: from Cargo.toml)

set -euo pipefail

cd "$(dirname "$0")"

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

# ── Parse args ─────────────────────────────────────────────────────
BUILD_ONLY=false
TAG_OVERRIDE=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --build-only) BUILD_ONLY=true; shift ;;
        --tag)        TAG_OVERRIDE="$2"; shift 2 ;;
        -h|--help)
            echo "Usage: ./release.sh [--build-only] [--tag vX.Y.Z]"
            exit 0
            ;;
        *) error "Unknown option: $1"; exit 1 ;;
    esac
done

# ── Determine version ─────────────────────────────────────────────
if [[ -n "$TAG_OVERRIDE" ]]; then
    VERSION="${TAG_OVERRIDE#v}"
    TAG="v$VERSION"
else
    VERSION=$(grep '^version' standalone/Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
    TAG="v$VERSION"
fi

info "Version: $VERSION (tag: $TAG)"

# ── Preflight checks ──────────────────────────────────────────────
header "Preflight checks"

if ! command -v gh &>/dev/null; then
    error "GitHub CLI (gh) not found. Install: https://cli.github.com/"
    exit 1
fi

if ! command -v cargo-deb &>/dev/null; then
    info "Installing cargo-deb..."
    cargo install cargo-deb
fi

if ! command -v dpkg-deb &>/dev/null; then
    error "dpkg-deb not found. Install: sudo apt install dpkg"
    exit 1
fi

# Check for uncommitted changes
if [[ -n "$(git status --porcelain)" ]]; then
    warn "Working directory has uncommitted changes."
    if [[ "$BUILD_ONLY" = false ]]; then
        error "Commit or stash changes before creating a release."
        exit 1
    fi
fi

info "All checks passed"

# ── Build packages ─────────────────────────────────────────────────
header "Building standalone app .deb"
cargo deb -p draytek-vpn
APP_DEB="target/debian/draytek-vpn-standalone_${VERSION}_amd64.deb"
if [[ ! -f "$APP_DEB" ]]; then
    APP_DEB=$(ls -t target/debian/draytek-vpn-standalone_*.deb 2>/dev/null | head -1)
fi
info "Built: $APP_DEB"

header "Building standalone app AppImage"
bash standalone/build_appimage.sh
APP_APPIMAGE="draytek-vpn-standalone_${VERSION}_x86_64.AppImage"
info "Built: $APP_APPIMAGE"

header "Building NetworkManager plugin .deb"
bash networkmanager/build_deb.sh
NM_DEB="target/deb-nm/draytek-vpn-networkmanager_${VERSION}_amd64.deb"
info "Built: $NM_DEB"

# ── Summary ────────────────────────────────────────────────────────
header "Build artifacts"
echo ""
ls -lh "$APP_DEB"
ls -lh "$APP_APPIMAGE"
ls -lh "$NM_DEB"
echo ""

if [[ "$BUILD_ONLY" = true ]]; then
    info "Build-only mode — skipping release creation."
    exit 0
fi

# ── Create GitHub release ──────────────────────────────────────────
header "Creating GitHub release"

# Check if tag already exists
if git rev-parse "$TAG" &>/dev/null; then
    error "Tag $TAG already exists. Use --tag to specify a different version."
    exit 1
fi

info "Tagging $TAG"
git tag "$TAG"
git push origin "$TAG"

info "Creating release on GitHub"
gh release create "$TAG" \
    "$APP_DEB" \
    "$APP_APPIMAGE" \
    "$NM_DEB" \
    --title "$TAG — DrayTek SSL VPN Client for Linux" \
    --generate-notes

RELEASE_URL=$(gh release view "$TAG" --json url -q '.url')
echo ""
info "Release published: $RELEASE_URL"
