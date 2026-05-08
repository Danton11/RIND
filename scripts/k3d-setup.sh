#!/usr/bin/env bash
set -euo pipefail

# k3d setup script for RIND DNS server.
# Creates a local k3d cluster, builds the image with kubernetes support,
# imports it, and applies the kustomize manifests.
#
# Usage:
#   ./scripts/k3d-setup.sh          # full setup
#   ./scripts/k3d-setup.sh build    # rebuild and redeploy only
#   ./scripts/k3d-setup.sh teardown # destroy cluster

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

CLUSTER_NAME="${RIND_K3D_CLUSTER:-rind-dev}"
IMAGE_NAME="${RIND_IMAGE:-rind:k8s}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()  { echo -e "${GREEN}[rind-k3d]${NC} $*"; }
warn() { echo -e "${YELLOW}[rind-k3d]${NC} $*"; }
err()  { echo -e "${RED}[rind-k3d]${NC} $*" >&2; }

check_prerequisites() {
    local missing=()
    command -v docker >/dev/null 2>&1 || missing+=("docker")
    command -v k3d >/dev/null 2>&1    || missing+=("k3d")
    command -v kubectl >/dev/null 2>&1 || missing+=("kubectl")

    if [ ${#missing[@]} -gt 0 ]; then
        err "Missing required tools: ${missing[*]}"
        err "Run ./scripts/install-prereqs.sh to install them, then re-run this script."
        exit 1
    fi
}

create_cluster() {
    if k3d cluster list 2>/dev/null | grep -q "$CLUSTER_NAME"; then
        log "Cluster '$CLUSTER_NAME' already exists, skipping creation"
        return
    fi

    log "Creating k3d cluster '$CLUSTER_NAME'..."
    k3d cluster create "$CLUSTER_NAME" \
        --port "30053:30053/udp@server:0" \
        --port "30080:30080/tcp@server:0" \
        --port "30090:30090/tcp@server:0" \
        --port "31000:31000/tcp@server:0" \
        --port "31090:31090/tcp@server:0" \
        --port "31093:31093/tcp@server:0" \
        --agents 1 \
        --k3s-arg "--kubelet-arg=eviction-hard=imagefs.available<1%,nodefs.available<1%@server:0" \
        --k3s-arg "--kubelet-arg=eviction-hard=imagefs.available<1%,nodefs.available<1%@agent:0" \
        --wait

    log "Cluster created. Waiting for node readiness..."
    kubectl wait --for=condition=Ready nodes --all --timeout=60s
}

build_image() {
    # CI builds the image with buildx + remote layer cache before calling
    # this script, then sets RIND_SKIP_BUILD=1 so we don't rebuild from
    # scratch inside the workflow.
    if [ "${RIND_SKIP_BUILD:-0}" = "1" ]; then
        log "RIND_SKIP_BUILD=1 — assuming '$IMAGE_NAME' is already in the local docker daemon"
        if ! docker image inspect "$IMAGE_NAME" >/dev/null 2>&1; then
            err "RIND_SKIP_BUILD=1 but image '$IMAGE_NAME' is not present"
            exit 1
        fi
        return
    fi
    log "Building RIND image with kubernetes feature..."
    docker build \
        -t "$IMAGE_NAME" \
        --build-arg FEATURES=kubernetes \
        -f "$PROJECT_DIR/docker/Dockerfile" \
        "$PROJECT_DIR"
}

import_image() {
    log "Importing image into k3d cluster..."
    k3d image import "$IMAGE_NAME" -c "$CLUSTER_NAME"
}

apply_manifests() {
    # When the image tag is unchanged but the underlying image content has
    # been re-imported (e.g. `./k3d-setup.sh build`), `kubectl apply` is a
    # no-op and pods keep running the old image. Detect that case (deployment
    # already existed) and force a rollout after apply so updates actually
    # deploy.
    local existed=0
    kubectl get deployment/rind -n rind-system >/dev/null 2>&1 && existed=1

    log "Applying kustomize manifests (k3d overlay)..."
    kubectl apply -k "$PROJECT_DIR/k8s/overlays/k3d/"

    if [ "$existed" -eq 1 ]; then
        log "Deployment already existed — forcing rollout to pick up the new image."
        kubectl rollout restart deployment/rind -n rind-system >/dev/null
    fi

    log "Waiting for deployment rollout..."
    kubectl rollout status deployment/rind -n rind-system --timeout=120s
}

show_status() {
    echo ""
    log "=== RIND k3d Deployment Status ==="
    echo ""
    kubectl get pods -n rind-system -o wide
    echo ""
    kubectl get svc -n rind-system
    echo ""
    kubectl get dnsrecords -n rind-system 2>/dev/null || true
    echo ""
    log "=== Access Points ==="
    log "  DNS:         dig @localhost -p 30053 <name>"
    log "  API:         curl http://localhost:30080/records"
    log "  Metrics:     curl http://localhost:30090/metrics"
    log "  Grafana:     http://localhost:31000  (admin / rind)  [after monitoring install]"
    log "  Prometheus:  http://localhost:31090                   [after monitoring install]"
    echo ""
    log "=== Quick Test ==="
    log "  kubectl apply -f $PROJECT_DIR/k8s/examples/sample-records.yaml"
    log "  dig @localhost -p 30053 www.example.com"
}

teardown() {
    log "Deleting k3d cluster '$CLUSTER_NAME'..."
    k3d cluster delete "$CLUSTER_NAME"
    log "Cluster deleted."
}

# --- Main ---

check_prerequisites

case "${1:-setup}" in
    setup)
        create_cluster
        build_image
        import_image
        apply_manifests
        show_status
        ;;
    build)
        build_image
        import_image
        apply_manifests
        show_status
        ;;
    teardown|destroy|delete)
        teardown
        ;;
    status)
        show_status
        ;;
    *)
        err "Unknown command: $1"
        echo "Usage: $0 [setup|build|teardown|status]"
        exit 1
        ;;
esac
