# Running RIND on Kubernetes

This doc is a stub. The full k8s story (Helm chart, MetalLB, HPA, source-IP
preservation) will be filled in as that work lands.

The one thing worth documenting now, because it's a footgun at deploy time
rather than at development time, is the storage layer.

## Storage

RIND uses LMDB as its embedded datastore. LMDB is memory-mapped and imposes
constraints most DB-backed apps don't.

### Filesystem requirements

LMDB needs a filesystem with working `mmap`, `fcntl` byte-range locking, and
`fsync`. In k8s terms:

| Storage type | Works? | Examples |
|---|---|---|
| Local disk | ✅ | `emptyDir`, `hostPath`, `local-path-provisioner` |
| Block-level network | ✅ | AWS EBS, GCP PD, Azure Disk, Ceph RBD, Longhorn, OpenEBS |
| File-level network | ❌ | AWS EFS, GCP Filestore, Azure Files, NFS, CephFS, GlusterFS, JuiceFS |

The restriction is LMDB's, not ours. File-level network storage gives weak
`mmap` page cache coherency and unreliable `fcntl` lock recovery after client
crashes — causing silent corruption and pod crash-loops after restarts.

Note that "PersistentVolume" does not imply "network filesystem." Most cloud
default StorageClasses are block-level and work fine. To check your own, look
at the CSI driver:

```bash
kubectl get storageclass -o custom-columns=NAME:.metadata.name,PROVISIONER:.provisioner
```

- `ebs.csi.aws.com` — block, fine
- `pd.csi.storage.gke.io` — block, fine
- `disk.csi.azure.com` — block, fine
- `efs.csi.aws.com` — file, broken
- `file.csi.azure.com` — file, broken

### Pod topology

Once the control-plane/data-plane split lands:

- **Control plane** — `StatefulSet` with a PVC on a block StorageClass. This
  is the authoritative source of truth. One replica (leader election can be
  added later if HA is needed).
- **DNS nodes** — `Deployment` with `emptyDir`. On pod start, each node
  bootstraps its LMDB from the control plane's sync API. Pod restart → empty
  disk → re-sync. Makes DNS nodes cattle, not pets, and unblocks HPA.

### LMDB map size vs pod memory limit

LMDB declares a maximum map size at env-open time. This is address space
reservation, not RSS — actual resident memory tracks the working set. But the
resident portion does count against the pod's memory limit.

Configure via `RIND_LMDB_MAP_SIZE` (byte count, default 1 GiB). Set the pod
memory limit based on expected working set, not map size.

### Cross-architecture snapshots

LMDB files are not portable across CPU architectures or endianness — pointer
size and byte order bake into the page layout. **Never ship raw `.mdb` files
between pods.** If a DNS node needs to catch up after falling behind the
changelog, it pulls a *logical* snapshot (bincode-over-HTTP) from the control
plane, not a file copy. This is already how the sync API is designed.
