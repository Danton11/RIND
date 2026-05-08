#!/usr/bin/env bash
set -euo pipefail

# Declarative install of the monitoring stack into the k3d cluster.
# All configuration lives in the values files next to this script.
#
# Usage: ./k8s/monitoring/install.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

GREEN='\033[0;32m'
NC='\033[0m'
log() { echo -e "${GREEN}[monitoring]${NC} $*"; }

# Ensure helm repos are added
log "Adding helm repos..."
helm repo add prometheus-community https://prometheus-community.github.io/helm-charts 2>/dev/null || true
helm repo add grafana https://grafana.github.io/helm-charts 2>/dev/null || true
helm repo update

# Install kube-prometheus-stack (Prometheus + Grafana + Alertmanager)
log "Installing kube-prometheus-stack..."
helm upgrade --install monitoring prometheus-community/kube-prometheus-stack \
  --namespace monitoring --create-namespace \
  -f "$SCRIPT_DIR/values.yaml" \
  --wait --timeout 5m

# Install Loki + Promtail (log aggregation)
log "Installing Loki stack..."
helm upgrade --install loki grafana/loki-stack \
  --namespace monitoring \
  -f "$SCRIPT_DIR/loki-values.yaml" \
  --wait --timeout 3m

# Apply ServiceMonitor + dashboard ConfigMap via kustomize
log "Applying RIND ServiceMonitor and dashboards..."
kubectl apply -k "$SCRIPT_DIR"

# Show status
log ""
log "=== Monitoring Stack Installed ==="
log ""
log "  Grafana:      http://localhost:31000  (admin / rind)"
log "  Prometheus:   http://localhost:31090"
log "  Alertmanager: http://localhost:31093"
log ""
log "  These ports are exposed by k3d-setup.sh. If your cluster was created"
log "  without them, use port-forward instead:"
log "    kubectl port-forward -n monitoring svc/monitoring-grafana 3000:80"
log "    kubectl port-forward -n monitoring svc/monitoring-kube-prometheus-prometheus 9091:9090"
log ""
log "  Loki is available as a Grafana data source."
log "  To add it manually: URL = http://loki.monitoring:3100"
log ""
log "  RIND metrics are scraped every 10s via the ServiceMonitor."
