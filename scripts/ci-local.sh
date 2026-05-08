#!/usr/bin/env bash
set -euo pipefail

# Run the CI checks locally without the GitHub Actions runtime. Mirrors
# `.github/workflows/ci.yml` step-for-step — keep both in sync if you edit
# one. For YAML-level workflow validation (action pin checks, matrix
# expansion), use `act` instead (`brew install act` / `pacman -S act`).
#
# Usage:
#   ./scripts/ci-local.sh rust         # fmt + clippy + test, both feature flags
#   ./scripts/ci-local.sh shellcheck   # bash lint + migrate fixture test
#   ./scripts/ci-local.sh manifests    # helm lint + kubeconform + kustomize
#   ./scripts/ci-local.sh smoke        # full k3d roundtrip (slowest)
#   ./scripts/ci-local.sh all          # everything, in CI order

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TOOLS_DIR="$PROJECT_DIR/.ci-tools"

CYAN='\033[0;36m'
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

step() { echo -e "\n${CYAN}==> $*${NC}"; }
ok()   { echo -e "${GREEN}✓ $*${NC}"; }
fail() { echo -e "${RED}✗ $*${NC}" >&2; exit 1; }
warn() { echo -e "${YELLOW}! $*${NC}" >&2; }

# Project-local cache for one-off CI binaries (kubeconform). System tools
# (cargo, kubectl, k3d, helm, shellcheck) come from install-prereqs.sh.
ensure_kubeconform() {
    if command -v kubeconform >/dev/null 2>&1; then
        KUBECONFORM=kubeconform
        return
    fi
    if [ -x "$TOOLS_DIR/kubeconform" ]; then
        KUBECONFORM="$TOOLS_DIR/kubeconform"
        return
    fi
    step "Installing kubeconform into $TOOLS_DIR (one-time)"
    mkdir -p "$TOOLS_DIR"
    local os arch url
    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    case "$(uname -m)" in
        x86_64|amd64) arch=amd64 ;;
        aarch64|arm64) arch=arm64 ;;
        *) fail "unsupported arch: $(uname -m)" ;;
    esac
    url="https://github.com/yannh/kubeconform/releases/download/v0.6.7/kubeconform-${os}-${arch}.tar.gz"
    curl -sfL "$url" | tar -xz -C "$TOOLS_DIR" kubeconform
    KUBECONFORM="$TOOLS_DIR/kubeconform"
}

require() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        fail "missing tool: $cmd — run ./scripts/install-prereqs.sh"
    fi
}

# --- jobs (mirror ci.yml) -------------------------------------------------

run_rust() {
    require cargo
    step "cargo fmt --check"
    cargo fmt --check
    step "cargo clippy --all-targets -- -D warnings"
    cargo clippy --all-targets -- -D warnings
    step "cargo clippy --all-targets --features kubernetes -- -D warnings"
    cargo clippy --all-targets --features kubernetes -- -D warnings
    step "cargo test"
    cargo test
    step "cargo test --features kubernetes"
    cargo test --features kubernetes
    ok "rust"
}

run_shellcheck() {
    require shellcheck
    # Scoped to scripts owned by this project's kubernetes work. Legacy
    # docker-compose scripts (start-canary.sh, start-fullstack.sh, ...) have
    # pre-existing warnings outside the scope of this CI; touch them in a
    # dedicated cleanup PR if at all.
    local scripts=(
        ci-local.sh
        eks-setup.sh
        install-prereqs.sh
        k3d-setup.sh
        migrate-to-crd.sh
        test-migrate-to-crd.sh
    )
    step "shellcheck (k8s scripts)"
    ( cd "$PROJECT_DIR/scripts" && shellcheck "${scripts[@]}" )
    step "scripts/test-migrate-to-crd.sh"
    bash "$PROJECT_DIR/scripts/test-migrate-to-crd.sh"
    ok "shellcheck"
}

run_manifests() {
    require helm
    require kubectl
    ensure_kubeconform

    step "helm lint charts/rind"
    helm lint "$PROJECT_DIR/charts/rind"

    # Helm emits stderr warnings about symlinks; pipe stdout only so they
    # don't mix into kubeconform's input.
    step "helm template + kubeconform"
    helm template rind "$PROJECT_DIR/charts/rind" 2>/dev/null \
        | "$KUBECONFORM" -strict -summary -skip DnsRecord -ignore-missing-schemas

    step "kustomize render k8s/base + kubeconform"
    kubectl kustomize "$PROJECT_DIR/k8s/base/" \
        | "$KUBECONFORM" -strict -summary -skip DnsRecord -ignore-missing-schemas

    step "kustomize render k8s/overlays/k3d + kubeconform"
    kubectl kustomize "$PROJECT_DIR/k8s/overlays/k3d/" \
        | "$KUBECONFORM" -strict -summary -skip DnsRecord -ignore-missing-schemas

    # EKS overlay references a placeholder ECR image that scripts/eks-setup.sh
    # rewrites at deploy time. Validate structural correctness only.
    step "kustomize render k8s/overlays/eks (structural)"
    kubectl kustomize "$PROJECT_DIR/k8s/overlays/eks/" \
        | "$KUBECONFORM" -summary -skip DnsRecord -ignore-missing-schemas

    ok "manifests"
}

run_smoke() {
    require docker
    require k3d
    require kubectl
    require dig
    require curl
    require jq

    # Reuse an existing cluster if there is one — this is the dev-loop ergonomic
    # difference between this script and CI. CI always starts cold.
    if k3d cluster list 2>/dev/null | grep -q '^rind-dev '; then
        step "Reusing existing rind-dev cluster (run teardown manually for a cold run)"
        # Re-applying manifests against the already-imported image is cheap.
        kubectl apply -k "$PROJECT_DIR/k8s/overlays/k3d/" >/dev/null
        kubectl rollout status deployment/rind -n rind-system --timeout=120s
    else
        step "Bringing up k3d cluster from scratch"
        "$PROJECT_DIR/scripts/k3d-setup.sh" setup
    fi

    step "Apply sample records"
    kubectl apply -f "$PROJECT_DIR/k8s/examples/sample-records.yaml"

    step "Wait for all CRDs to sync (status.synced=true)"
    local unsynced
    for _ in $(seq 1 30); do
        unsynced=$(kubectl get dnsrecord -n rind-system -o json \
            | jq '[.items[] | select(.status.synced != true)] | length')
        [ "$unsynced" = "0" ] && break
        sleep 1
    done
    [ "$unsynced" = "0" ] || {
        kubectl get dnsrecord -n rind-system >&2
        fail "$unsynced CRDs failed to sync within 30s"
    }

    step "dig 9 record types"
    assert_resolves() {
        local name="$1" qtype="$2" expected="$3" actual
        actual=$(dig @localhost -p 30053 "$name" "$qtype" +short)
        if [[ "$actual" != *"$expected"* ]]; then
            fail "dig $qtype $name: expected '$expected', got '$actual'"
        fi
        echo "    $qtype $name → $actual"
    }
    assert_resolves www.example.com           A     "10.0.1.50"
    assert_resolves api.example.com           A     "10.0.1.100"
    assert_resolves example.com               MX    "mail.example.com"
    assert_resolves example.com               NS    "ns1.example.com"
    assert_resolves example.com               TXT   "v=spf1"
    assert_resolves example.com               SOA   "ns1.example.com"
    assert_resolves app.example.com           CNAME "www.example.com"
    assert_resolves _sip._tcp.example.com     SRV   "sipserver.example.com"

    step "REST CRUD roundtrip + RRset 409"
    local status id actual rc
    status=$(curl -sS -o /tmp/ci-local.post.json -w '%{http_code}' \
        -X POST http://localhost:30080/records \
        -H 'Content-Type: application/json' \
        -d '{"name":"smoke.example.com","ttl":120,"type":"A","ip":"203.0.113.99"}')
    [ "$status" = "201" ] || { cat /tmp/ci-local.post.json >&2; fail "POST status=$status"; }
    id=$(jq -r '.data.id' /tmp/ci-local.post.json)
    echo "    POST 201, id=$id"

    status=$(curl -sS -o /tmp/ci-local.conflict.json -w '%{http_code}' \
        -X POST http://localhost:30080/records \
        -H 'Content-Type: application/json' \
        -d '{"name":"smoke.example.com","type":"CNAME","target":"foo.test"}')
    [ "$status" = "409" ] || { cat /tmp/ci-local.conflict.json >&2; fail "expected 409 on CNAME-over-A, got $status"; }
    echo "    POST 409 on CNAME-over-A (RRset shim)"

    actual=""
    for _ in $(seq 1 10); do
        actual=$(dig @localhost -p 30053 smoke.example.com +short)
        [ "$actual" = "203.0.113.99" ] && break
        sleep 1
    done
    [ "$actual" = "203.0.113.99" ] || fail "dig got '$actual', expected 203.0.113.99"
    echo "    dig smoke.example.com → 203.0.113.99"

    status=$(curl -sS -o /dev/null -w '%{http_code}' -X DELETE \
        "http://localhost:30080/records/$id")
    [ "$status" = "204" ] || fail "DELETE status=$status"
    rc=""
    for _ in $(seq 1 10); do
        rc=$(dig @localhost -p 30053 smoke.example.com 2>&1 \
            | grep -oE 'NXDOMAIN|NOERROR' | head -1 || true)
        [ "$rc" = "NXDOMAIN" ] && break
        sleep 1
    done
    [ "$rc" = "NXDOMAIN" ] || fail "expected NXDOMAIN after delete, got '$rc'"
    echo "    DELETE 204, dig → NXDOMAIN"

    step "/health"
    curl -sS http://localhost:30080/health | jq -e '.ready == true' >/dev/null \
        || fail "/health did not return ready=true"
    echo "    /health → ready=true"

    ok "smoke"
}

usage() {
    cat <<EOF
Usage: $0 <command>

Commands:
  rust         cargo fmt + clippy (both feature flags) + test (both feature flags)
  shellcheck   shellcheck scripts/*.sh + migrate-to-crd fixture test
  manifests    helm lint + kubeconform + kustomize (base, k3d, eks)
  smoke        full k3d cluster smoke — setup if needed, apply samples,
               dig 9 record types, REST CRUD + RRset 409, /health
  all          rust + shellcheck + manifests + smoke, in CI order

This mirrors .github/workflows/ci.yml step-for-step. Edits to one should
be reflected in the other. For workflow-YAML validation (action pin
drift, matrix expansion), use \`act\` instead.
EOF
}

case "${1:-}" in
    rust)       run_rust ;;
    shellcheck) run_shellcheck ;;
    manifests)  run_manifests ;;
    smoke)      run_smoke ;;
    all)
        run_rust
        run_shellcheck
        run_manifests
        run_smoke
        ;;
    -h|--help|help|"")
        usage
        exit 0
        ;;
    *)
        fail "unknown command: $1"
        ;;
esac

echo
ok "all requested checks passed"
