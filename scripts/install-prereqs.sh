#!/usr/bin/env bash
set -euo pipefail

# Install the tools k3d-setup.sh and eks-setup.sh need.
# Detects the host package manager (pacman / apt / dnf / brew) and installs
# anything missing. Safe to re-run — already-installed tools are skipped.
#
# What gets installed:
#   - docker      (daemon — verifies presence; does not auto-start)
#   - kubectl
#   - k3d
#   - helm
#   - dig         (bind-utils / bind9-dnsutils / bind-tools, used for testing)
#   - shellcheck  (used by ci-local.sh and the shellcheck CI job)
#   - jq          (used by migrate-to-crd.sh and ci-local.sh)
#
# Optional (mentioned at end, not auto-installed):
#   - act         (run GitHub Actions workflows locally — only needed when
#                  editing .github/workflows/*.yml)
#
# Usage:
#   ./scripts/install-prereqs.sh

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()  { echo -e "${GREEN}[prereqs]${NC} $*"; }
warn() { echo -e "${YELLOW}[prereqs]${NC} $*"; }
err()  { echo -e "${RED}[prereqs]${NC} $*" >&2; }

detect_pm() {
    if   command -v brew    >/dev/null 2>&1; then echo brew
    elif command -v pacman  >/dev/null 2>&1; then echo pacman
    elif command -v apt-get >/dev/null 2>&1; then echo apt
    elif command -v dnf     >/dev/null 2>&1; then echo dnf
    else echo unknown
    fi
}

PM=$(detect_pm)
if [ "$PM" = "unknown" ]; then
    err "No supported package manager found (brew/pacman/apt/dnf)."
    err "Install kubectl, k3d, docker, and dig manually, then re-run k3d-setup.sh."
    exit 1
fi
log "Package manager: $PM"

install_pkg() {
    local pkg_pacman="$1" pkg_apt="$2" pkg_dnf="$3" pkg_brew="$4"
    case "$PM" in
        pacman) sudo pacman -S --needed --noconfirm "$pkg_pacman" ;;
        apt)    sudo apt-get install -y "$pkg_apt" ;;
        dnf)    sudo dnf install -y "$pkg_dnf" ;;
        brew)   brew install "$pkg_brew" ;;
    esac
}

# Skip everything if all four tools are already on PATH. This makes the
# script cheap to re-run and lets k3d-setup.sh recommend it unconditionally.
if command -v docker     >/dev/null 2>&1 \
&& command -v kubectl    >/dev/null 2>&1 \
&& command -v k3d        >/dev/null 2>&1 \
&& command -v helm       >/dev/null 2>&1 \
&& command -v dig        >/dev/null 2>&1 \
&& command -v shellcheck >/dev/null 2>&1 \
&& command -v jq         >/dev/null 2>&1; then
    log "All prerequisites already installed."
    log "  docker:     $(docker --version 2>/dev/null | head -1)"
    log "  kubectl:    $(kubectl version --client 2>/dev/null | head -1)"
    log "  k3d:        $(k3d version 2>/dev/null | head -1)"
    log "  helm:       $(helm version --short 2>/dev/null)"
    log "  dig:        present"
    log "  shellcheck: $(shellcheck --version 2>/dev/null | awk '/version:/ {print $2}')"
    log "  jq:         $(jq --version 2>/dev/null)"
    if command -v act >/dev/null 2>&1; then
        log "  act:        $(act --version 2>/dev/null) (optional)"
    else
        log "  act:        not installed (optional — install only if editing .github/workflows/*)"
    fi
    exit 0
fi

# Refresh package db once up front. apt and pacman need this to avoid 404s on
# stale mirrors; dnf and brew refresh on each install.
case "$PM" in
    pacman) log "Refreshing pacman db..."; sudo pacman -Sy --noconfirm ;;
    apt)    log "Refreshing apt db...";    sudo apt-get update -y ;;
esac

# --- docker ---------------------------------------------------------------
if command -v docker >/dev/null 2>&1; then
    log "docker: already installed ($(docker --version))"
else
    log "Installing docker..."
    install_pkg docker docker.io docker docker
    if [ "$PM" != "brew" ]; then
        warn "Enable + start the docker daemon and add yourself to the docker group:"
        warn "  sudo systemctl enable --now docker"
        warn "  sudo usermod -aG docker \$USER  &&  newgrp docker"
    fi
fi
if ! docker ps >/dev/null 2>&1; then
    warn "Docker is installed but the daemon isn't reachable. Start it before running k3d-setup.sh."
fi

# --- kubectl --------------------------------------------------------------
if command -v kubectl >/dev/null 2>&1; then
    log "kubectl: already installed ($(kubectl version --client 2>/dev/null | head -1))"
else
    log "Installing kubectl..."
    install_pkg kubectl kubectl kubectl kubectl
fi

# --- helm -----------------------------------------------------------------
# Needed by k8s/monitoring/install.sh and scripts/eks-setup.sh.
if command -v helm >/dev/null 2>&1; then
    log "helm: already installed ($(helm version --short 2>/dev/null))"
else
    log "Installing helm..."
    install_pkg helm helm helm helm
fi

# --- k3d ------------------------------------------------------------------
# k3d isn't in the major Linux package repos; use the upstream installer.
if command -v k3d >/dev/null 2>&1; then
    log "k3d: already installed ($(k3d version | head -1))"
else
    log "Installing k3d via upstream installer..."
    if [ "$PM" = "brew" ]; then
        brew install k3d
    else
        curl -fsSL https://raw.githubusercontent.com/k3d-io/k3d/main/install.sh | sudo bash
    fi
fi

# --- dig (DNS query tool, used for smoke tests) ---------------------------
if command -v dig >/dev/null 2>&1; then
    log "dig: already installed"
else
    log "Installing dig..."
    install_pkg bind bind9-dnsutils bind-utils bind
fi

# --- shellcheck (used by ci-local.sh shellcheck job) ----------------------
if command -v shellcheck >/dev/null 2>&1; then
    log "shellcheck: already installed"
else
    log "Installing shellcheck..."
    install_pkg shellcheck shellcheck ShellCheck shellcheck
fi

# --- jq (used by migrate-to-crd.sh and the smoke job) ---------------------
if command -v jq >/dev/null 2>&1; then
    log "jq: already installed"
else
    log "Installing jq..."
    install_pkg jq jq jq jq
fi

log "All prerequisites installed."
log
log "Optional: install \`act\` if you want to run GitHub Actions workflows"
log "locally (only useful when editing .github/workflows/*.yml). Otherwise"
log "use ./scripts/ci-local.sh for the same checks at lower overhead."
log
log "Next: ./scripts/k3d-setup.sh"
