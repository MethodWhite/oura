#!/usr/bin/env bash
set -euo pipefail

REPO="MethodWhite/oura"
VERSION="${1:-latest}"
INSTALL_DIR="${OURA_INSTALL_DIR:-/usr/local/bin}"
CONFIG_DIR="${OURA_CONFIG_DIR:-$HOME/.config/oura}"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

log()  { printf "${GREEN}[✓]${NC} %s\n" "$*"; }
warn() { printf "${YELLOW}[!]${NC} %s\n" "$*"; }
err()  { printf "${RED}[✗]${NC} %s\n" "$*"; exit 1; }
info() { printf "${CYAN}[i]${NC} %s\n" "$*"; }

detect_arch() {
  local arch
  arch=$(uname -m)
  case "$arch" in
    x86_64|amd64) echo "x86_64" ;;
    aarch64|arm64) echo "aarch64" ;;
    *) err "Unsupported architecture: $arch" ;;
  esac
}

detect_os() {
  local os
  os=$(uname -s)
  case "$os" in
    Linux)  echo "unknown-linux-gnu" ;;
    Darwin) echo "apple-darwin" ;;
    *)      err "Unsupported OS: $os" ;;
  esac
}

get_release_url() {
  local os_arch="$1"
  if [ "$VERSION" = "latest" ]; then
    echo "https://github.com/$REPO/releases/latest/download/oura-${os_arch}"
  else
    echo "https://github.com/$REPO/releases/download/$VERSION/oura-${os_arch}"
  fi
}

main() {
  echo ""
  printf "${CYAN}╔══════════════════════════════════════╗${NC}\n"
  printf "${CYAN}║      Oura Installer — 0XFFRice       ║${NC}\n"
  printf "${CYAN}╚══════════════════════════════════════╝${NC}\n"
  echo ""

  if [ "$(id -u)" -eq 0 ]; then
    warn "Running as root — installing system-wide to $INSTALL_DIR"
  else
    info "Running as user — installing to $INSTALL_DIR"
    if ! mkdir -p "$INSTALL_DIR" 2>/dev/null; then
      info "Need sudo to write to $INSTALL_DIR"
      exec sudo "$0" "$@"
    fi
  fi

  local arch vendor os_arch
  arch=$(detect_arch)
  vendor=$(detect_os)
  os_arch="${arch}-${vendor}"
  local url
  url=$(get_release_url "$os_arch")

  info "Detected: $os_arch"
  info "Download: $url"

  local tmpdir
  tmpdir=$(mktemp -d)
  trap 'rm -rf "$tmpdir"' EXIT

  if command -v curl &>/dev/null; then
    curl -fsSL "$url" -o "$tmpdir/oura" || err "Download failed"
  elif command -v wget &>/dev/null; then
    wget -q "$url" -O "$tmpdir/oura" || err "Download failed"
  else
    err "Need curl or wget"
  fi

  chmod +x "$tmpdir/oura"
  "$tmpdir/oura" --version &>/dev/null || err "Binary validation failed"

  mkdir -p "$INSTALL_DIR"
  cp "$tmpdir/oura" "$INSTALL_DIR/oura"
  log "Installed: $INSTALL_DIR/oura"

  mkdir -p "$CONFIG_DIR"
  if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    cat > "$CONFIG_DIR/config.toml" << 'TOML'
[loop_engine]
max_iterations = 20
convergence_threshold = 90.0
feedback_sources = ["test", "lint"]

[github]
enabled = true
default_owner = "MethodWhite"
default_repo = "my-project"
auto_commit = true
auto_pr = true

# [synapsis]  # Optional: uncomment for Synapsis integration (separate project)
# enabled = true
# endpoint = "http://localhost:7438"
TOML
    log "Config created: $CONFIG_DIR/config.toml"
  else
    warn "Config exists at $CONFIG_DIR/config.toml — skipping"
  fi

  if echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    log "Ready to use. Run: oura --help"
  else
    warn "$INSTALL_DIR is not in PATH. Add it:"
    warn "  export PATH=\"\$PATH:$INSTALL_DIR\""
  fi

  echo ""
  log "Oura installed successfully!"
  "$INSTALL_DIR/oura" version 2>/dev/null || true
  echo ""
}

main "$@"
