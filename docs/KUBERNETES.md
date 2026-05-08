# Running RIND on Kubernetes

RIND runs natively on Kubernetes using a `DnsRecord` Custom Resource Definition.
etcd (via the K8s API) is the authoritative data store; each RIND pod maintains
a local LMDB cache for fast DNS query resolution.

## Architecture

```
Users/CI ──kubectl apply──► K8s API (etcd) ──watch stream──► RIND Pod A ──► LMDB ──► DNS
                                            ──watch stream──► RIND Pod B ──► LMDB ──► DNS
                                            ──watch stream──► RIND Pod N ──► LMDB ──► DNS
```

- **All pods are equal** — no leader, no follower, no write-forwarding
- **DNS queries** hit local LMDB only (no K8s API in the hot path)
- **Writes** go through `kubectl apply` or the REST API shim (which proxies to K8s API)
- **Consistency** is eventual (~50-200ms propagation) which matches DNS TTL semantics

## Prerequisites

Install the host tools (`docker`, `kubectl`, `k3d`, `dig`):

```bash
./scripts/install-prereqs.sh
```

The script detects pacman / apt / dnf / brew. Re-run safely; already-installed
tools are skipped. Make sure the docker daemon is running before continuing.

## Quick Start (k3d)

```bash
# One command spins up a local cluster with RIND
./scripts/k3d-setup.sh

# Create a DNS record
kubectl apply -f k8s/examples/sample-records.yaml

# Query it
dig @localhost -p 30053 www.example.com

# Or use the REST API
curl http://localhost:30080/records
```

## Quick Start (EKS)

```bash
# Creates cluster, ECR repo, pushes image, deploys
./scripts/eks-setup.sh

# Get the NLB endpoint
kubectl get svc rind-dns -n rind-system

# Create records
kubectl apply -f k8s/examples/sample-records.yaml
```

## DnsRecord CRD

Records are declared as Kubernetes objects:

```yaml
apiVersion: dns.rind.dev/v1alpha1
kind: DnsRecord
metadata:
  name: my-web-server
  namespace: rind-system
spec:
  name: www.example.com
  ttl: 300
  class: IN
  recordData:
    type: A
    ip: "10.0.1.50"
```

Supported record types: `A`, `AAAA`, `CNAME`, `PTR`, `NS`, `MX`, `TXT`, `SOA`, `SRV`.

### Field reference

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `spec.name` | yes | — | DNS name |
| `spec.ttl` | no | 300 | TTL in seconds (max 604800) |
| `spec.class` | no | IN | DNS class (IN, CH, HS) |
| `spec.recordData.type` | yes | — | Record type |
| `spec.recordData.ip` | A/AAAA | — | IP address |
| `spec.recordData.target` | CNAME/PTR/NS | — | Target hostname |
| `spec.recordData.preference` | MX | — | MX priority |
| `spec.recordData.exchange` | MX | — | MX exchange host |
| `spec.recordData.strings` | TXT | — | TXT string array |
| `spec.recordData.{mname,rname,serial,refresh,retry,expire,minimum}` | SOA | — | SOA tuple (RFC 1035 §3.3.13) |
| `spec.recordData.{priority,weight,port,target}` | SRV | — | SRV tuple (RFC 2782) |

### Useful commands

```bash
# List all records
kubectl get dnsrecords -n rind-system

# Short form
kubectl get dr -n rind-system

# Filter by type
kubectl get dr -n rind-system -l dns.rind.dev/record-type=A

# Watch for changes
kubectl get dr -n rind-system -w
```

## Operating Modes

RIND supports two modes, controlled by the `RIND_MODE` environment variable:

| Mode | Authority | REST writes | Use case |
|------|-----------|-------------|----------|
| `standalone` (default) | LMDB | Direct to LMDB | Local dev, Docker Compose |
| `kubernetes` | etcd (CRD) | Proxy to K8s API | k3d, EKS, any K8s cluster |

In `kubernetes` mode:
- A CRD watcher syncs records from etcd → local LMDB on startup and continuously
- REST `POST`/`PUT`/`DELETE` calls create/patch/delete CRD objects via K8s API
- REST `GET` calls read from local LMDB (fast path)
- Periodic full resync every 5min as a safety net (configurable via `RIND_RESYNC_INTERVAL_SECS`)
- `GET /health` returns `503 {"ready": false}` until the watcher's first
  full sync completes, then `200 {"ready": true}`. The kubelet readiness
  probe hits this so fresh pods don't take traffic before LMDB is caught
  up; liveness stays a TCP check on the API port so transient etcd
  failures don't restart-loop a healthy process.

## Kustomize Structure

Manifests use Kustomize with a shared base and environment-specific overlays:

```
k8s/
├── base/              # Shared: CRD, RBAC, Deployment, Services, ConfigMap
├── overlays/
│   ├── k3d/          # NodePort, 1 replica, low resources
│   └── eks/          # NLB, PVC, HPA, IRSA, ECR image
└── examples/         # Sample DnsRecord CRDs
```

Apply with: `kubectl apply -k k8s/overlays/k3d/` or `kubectl apply -k k8s/overlays/eks/`

## Storage

LMDB is used as a local cache (read-only in kubernetes mode — only the watcher writes).
The same filesystem restrictions apply:

| Storage type | Works? | Examples |
|---|---|---|
| Local disk | yes | `emptyDir`, `hostPath`, `local-path-provisioner` |
| Block-level network | yes | AWS EBS, GCP PD, Azure Disk |
| File-level network | **no** | AWS EFS, NFS, CephFS |

For k3d: `emptyDir` is fine (pod restart re-syncs from etcd).
For EKS: EBS-backed PVC provides persistence across restarts (faster startup).

## Migrating from Standalone

If you have existing records in a standalone RIND instance:

```bash
# 1. Dump records and convert to CRD YAML
./scripts/migrate-to-crd.sh http://localhost:8080 > migrated.yaml

# 2. Review the output
less migrated.yaml

# 3. Apply to the cluster
kubectl apply -f migrated.yaml

# 4. Switch to kubernetes mode (update ConfigMap or env)
# Records will be re-synced from etcd on startup
```

Or do it in one shot:
```bash
./scripts/migrate-to-crd.sh --apply http://localhost:8080
```

## Configuration

| Env var | Default | Description |
|---------|---------|-------------|
| `RIND_MODE` | `standalone` | Operating mode (`standalone` or `kubernetes`) |
| `RIND_NAMESPACE` | `rind-system` | K8s namespace to watch for CRDs |
| `RIND_RESYNC_INTERVAL_SECS` | `300` | Full resync interval (seconds) |
| `RIND_LMDB_PATH` | `./lmdb` | LMDB data directory |
| `RIND_LMDB_MAP_SIZE` | `1073741824` | LMDB address space (bytes) |
| `DNS_BIND_ADDR` | `127.0.0.1:12312` | DNS UDP listen address |
| `API_BIND_ADDR` | `127.0.0.1:8080` | REST API listen address |
| `METRICS_PORT` | `9090` | Prometheus metrics port |

## Building with Kubernetes Support

The kubernetes feature is optional and adds ~20MB to the binary (kube-rs + dependencies):

```bash
# Without kubernetes (standalone only)
cargo build --release

# With kubernetes support
cargo build --release --features kubernetes

# Docker
docker build --build-arg FEATURES=kubernetes -f docker/Dockerfile .
```

## RBAC

RIND pods bind a namespaced `Role` with permissions to:
- Watch/CRUD `dnsrecords.dns.rind.dev` in the namespace they run in
- Update `dnsrecords/status` subresource
- Create events

See `k8s/base/role.yaml` for the full definition. The role is namespaced
because every code path uses `Api::namespaced` — making it cluster-wide
just widens the blast radius if credentials leak.

## Local CI

`./scripts/ci-local.sh` mirrors `.github/workflows/ci.yml` step-for-step
so you can validate changes before pushing:

```bash
./scripts/ci-local.sh rust         # fmt + clippy + test, both feature flags
./scripts/ci-local.sh shellcheck   # bash lint + migrate-to-crd fixture test
./scripts/ci-local.sh manifests    # helm lint + kubeconform + kustomize render
./scripts/ci-local.sh smoke        # full k3d cluster smoke
./scripts/ci-local.sh all          # everything, in CI order
```

The `smoke` subcommand reuses an existing `rind-dev` cluster if one is
running; otherwise it brings one up via `k3d-setup.sh`. CI always starts
cold and uses a buildx layer cache to keep image rebuilds under a minute
on warm runs.

## Troubleshooting

### `docker pull` times out / TLS handshake timeout to `registry-1.docker.io`

Some networks have broken outbound IPv6 to Docker Hub's registry endpoint.
Symptom: `auth.docker.io` works but `registry-1.docker.io` hangs. Pin the
registry to its IPv4 address as a workaround:

```bash
sudo sh -c 'getent ahostsv4 registry-1.docker.io | head -1 \
  | awk "{print \$1\" registry-1.docker.io\"}" >> /etc/hosts'
```

If that's the issue, `docker pull rancher/k3s:v1.31.5-k3s1` will work after.

### Pods stuck `ImagePullBackOff` for sidecar/canary images

The k3d nodes pull external images directly from Docker Hub. If your network
can't reach it from inside the cluster, only the locally-built `rind:k8s`
image will start; other images (e.g. `python:3.11-slim` for the canary) will
fail. Pull them on the host first and import:

```bash
docker pull python:3.11-slim
k3d image import python:3.11-slim -c rind-dev
```

### Watcher exits with `429 storage is (re)initializing`

The K8s API can briefly 429 during pod startup before its etcd is ready. The
watcher exits with code 1 and the kubelet restarts the pod automatically —
this should self-heal. If a pod loops, check kube-apiserver logs.
