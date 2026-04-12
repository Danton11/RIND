// `main.rs` declares `mod storage;` but only calls a handful of methods, so
// the bin crate's dead_code analysis flags the rest. The lib crate (used by
// tests) exercises them fine. Drop this attribute once main.rs is converted
// to consume `librind` instead of re-declaring modules.
#![allow(dead_code)]

//! LMDB-backed storage for DNS records and the replication changelog.
//!
//! # Architecture
//!
//! Each RIND process owns a single LMDB environment containing five databases:
//!
//! - `records` — authoritative record store, keyed by UUID
//! - `records_by_name` — secondary index keyed by `name\0type_be\0uuid`
//! - `zones` — authoritative zone metadata (SOA fields), keyed by zone name
//! - `changelog` — versioned mutation log, keyed by `u64_be` version
//! - `metadata` — env-level state (schema version, current version, rolling
//!   state hash)
//!
//! Writes mutate `records`, `records_by_name`, `changelog`, and `metadata`
//! atomically in a single `RwTxn`. Either all of them land or none of them do.
//! This is the core property that LMDB buys us over the JSONL backend.
//!
//! # Storage requirements (read before deploying)
//!
//! LMDB needs a filesystem with working `mmap`, `fcntl` locking, and `fsync`.
//! In practice that means:
//!
//! - ✅ Local disk (emptyDir, hostPath, local-path-provisioner)
//! - ✅ Block-level network storage — AWS EBS, GCP PD, Azure Disk, Ceph RBD,
//!   Longhorn, OpenEBS. LMDB sees a normal local filesystem on top of a block
//!   device; the network part is invisible to it.
//! - ❌ File-level network storage — NFS, AWS EFS, GCP Filestore, Azure Files,
//!   CephFS, GlusterFS, JuiceFS. These give weak mmap coherency and unreliable
//!   lock recovery after client crashes, which corrupts LMDB silently and
//!   crash-loops pods after restarts.
//!
//! The restriction is LMDB's, not ours. If unsure about your k8s StorageClass,
//! check the CSI driver: `ebs.csi.aws.com` is block (fine), `efs.csi.aws.com`
//! is file (broken). Most cloud default StorageClasses are block.
//!
//! **Cross-architecture note.** LMDB files are not portable across CPU
//! architectures or endianness — pointer size and byte order bake into the
//! page layout. Never ship raw `.mdb` files between pods. Replication happens
//! through the logical sync API, not file copies.
//!
//! # Schema versioning
//!
//! `metadata["schema_version"]` stores a `u16` format tag. On `open()`, if the
//! metadata db is empty we initialize it to [`SCHEMA_VERSION`]. If it exists
//! and doesn't match, we return [`StorageError::SchemaMismatch`] — crash-loop
//! in k8s beats silent misparse. A migration tool will live in a sibling
//! module once we actually need to evolve the schema.

use chrono::{DateTime, Utc};
use heed::{types::Bytes, Database, Env, EnvOpenOptions, RwTxn};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

use crate::update::DnsRecord;

/// On-disk format version. Bump when the layout of any database value changes
/// in a way readers can't tolerate. Entries serialized under an older version
/// will not be silently reinterpreted — `open()` fails fast.
pub const SCHEMA_VERSION: u16 = 1;

/// Upper bound on the number of named databases this env will ever hold.
///
/// heed requires declaring this at env-open time; raising it later means
/// closing and reopening the env. We currently use 5 (`records`,
/// `records_by_name`, `zones`, `changelog`, `metadata`) and reserve slots
/// for `changelog_by_record_id` (audit log queries), `records_by_type`
/// (filtered list endpoint), and future growth.
const MAX_DBS: u32 = 8;

/// Default LMDB map size if the env var is unset. 1 GiB of address space —
/// actual resident memory tracks working set, not this number.
const DEFAULT_MAP_SIZE_BYTES: usize = 1024 * 1024 * 1024;

/// Env var overriding the map size. Accepts a raw byte count.
pub const MAP_SIZE_ENV: &str = "RIND_LMDB_MAP_SIZE";

/// Metadata db keys. These are the only keys that will ever exist in the
/// metadata database; naming them as constants stops typos from becoming
/// silent lookup failures.
pub mod meta_keys {
    pub const SCHEMA_VERSION: &[u8] = b"schema_version";
    pub const CURRENT_VERSION: &[u8] = b"current_version";
    pub const STATE_HASH: &[u8] = b"state_hash";
}

/// All errors from the storage layer.
///
/// Callers can match specifically on `NotFound` (→ HTTP 404) and
/// `SchemaMismatch` (→ fail fast on startup). Everything else is an internal
/// error and maps to HTTP 500 at the API boundary.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("lmdb: {0}")]
    Heed(#[from] heed::Error),

    #[error("serde_json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("record not found: {0}")]
    NotFound(String),

    #[error("corrupt metadata: {0}")]
    CorruptMetadata(&'static str),

    #[error("schema mismatch: on-disk version {found}, binary expects {expected}")]
    SchemaMismatch { expected: u16, found: u16 },
}

/// The kind of mutation recorded in a single changelog op.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpKind {
    Create,
    Update,
    Delete,
}

/// A single record mutation. Multiple ops may share one changelog entry when
/// they were committed together (bulk writes produce one entry with N ops).
///
/// For `Create` and `Update`, `record` carries the post-mutation state. For
/// `Delete`, `record` is `None` and `record_id` identifies the removed row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordOp {
    pub kind: OpKind,
    pub record_id: String,
    pub record: Option<DnsRecord>,
}

/// A DNS zone this server is authoritative for. Mirrors the SOA record
/// fields from RFC 1035 §3.3.13 plus the canonical zone name.
///
/// The zone table exists to make RIND a *real* authoritative nameserver
/// later: AA flag, AUTHORITY section population, negative-cache TTL from
/// `minimum`, delegation cuts, REFUSED for out-of-zone queries — all of
/// that needs a zone concept first. Storing it from day one costs us
/// nothing now and avoids a painful schema migration later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zone {
    /// Canonical zone name (lowercase, no trailing dot).
    pub name: String,
    /// Primary master name server — SOA MNAME.
    pub mname: String,
    /// Responsible-party mailbox in DNS form — SOA RNAME.
    pub rname: String,
    /// Version of this zone's data. MUST be bumped on any change.
    pub serial: u32,
    /// Seconds a secondary waits before re-checking SOA.
    pub refresh: u32,
    /// Seconds a secondary waits before retrying a failed refresh.
    pub retry: u32,
    /// Seconds a secondary will keep serving stale data before expiring.
    pub expire: u32,
    /// Negative-cache TTL (RFC 2308). Controls how long resolvers cache
    /// NXDOMAIN / NODATA answers from this zone.
    pub minimum: u32,
}

/// One committed transaction in the changelog. Even single-record writes
/// produce an entry with `ops.len() == 1`; the `Vec` exists so bulk writes
/// don't force a wire-protocol break later.
///
/// `schema_version` is duplicated on every entry (not just in metadata) so a
/// sync client reading a streamed changelog can fail loudly on a mismatch
/// without needing to also fetch the env-level metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    pub schema_version: u16,
    pub version: u64,
    pub ts: DateTime<Utc>,
    pub ops: Vec<RecordOp>,
}

/// Owning handle to the LMDB environment and all its typed databases.
///
/// Cheap to clone: `heed::Env` is `Arc<EnvInner>` internally, so every clone
/// shares the same underlying environment. We deliberately do not wrap this
/// in `Arc<LmdbStore>` ourselves — that would add a second indirection for no
/// benefit.
#[derive(Clone)]
pub struct LmdbStore {
    env: Env,
    // All databases use raw byte keys/values. heed ships typed codecs
    // (e.g. `Database<Str, SerdeBincode<DnsRecord>>`) but the
    // `records_by_name` compound key (`name \0 type_be \0 uuid`) does not
    // fit any built-in codec. Rather than run two different codec systems
    // side-by-side, we encode everything by hand in one layer. Bincode for
    // values, hand-rolled byte layout for compound keys.
    records: Database<Bytes, Bytes>,
    records_by_name: Database<Bytes, Bytes>,
    zones: Database<Bytes, Bytes>,
    #[allow(dead_code)] // populated once changelog writes are wired in
    changelog: Database<Bytes, Bytes>,
    metadata: Database<Bytes, Bytes>,
}

impl LmdbStore {
    /// Open (or create) an LMDB environment at `path` and return a handle
    /// with all five databases ready for use.
    ///
    /// On first open the metadata db is seeded with `schema_version =
    /// SCHEMA_VERSION`, `current_version = 0`, and a zeroed state hash. On
    /// subsequent opens the on-disk schema version is checked against the
    /// binary's compiled `SCHEMA_VERSION` and mismatches fail loudly.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref();
        // LMDB expects the directory to exist before `open()`. Creating it
        // here keeps callers from having to `mkdir -p` before every open.
        std::fs::create_dir_all(path)?;

        let map_size = resolve_map_size()?;
        // SAFETY: `heed::EnvOpenOptions::open` is `unsafe` because it mmaps
        // a file. The contract: the file must not be concurrently modified
        // by any other process/environment outside LMDB's locking protocol.
        // We enforce that by owning the directory exclusively (StatefulSet
        // with one replica, or local tempdir in tests).
        let env = unsafe {
            EnvOpenOptions::new()
                .max_dbs(MAX_DBS)
                .map_size(map_size)
                .open(path)?
        };

        // One write txn covers db creation + metadata seed/check. Atomic —
        // a crash mid-seed leaves a completely uninitialised env, not a
        // half-initialised one.
        let mut wtxn = env.write_txn()?;
        let records = env.create_database::<Bytes, Bytes>(&mut wtxn, Some("records"))?;
        let records_by_name =
            env.create_database::<Bytes, Bytes>(&mut wtxn, Some("records_by_name"))?;
        let zones = env.create_database::<Bytes, Bytes>(&mut wtxn, Some("zones"))?;
        let changelog = env.create_database::<Bytes, Bytes>(&mut wtxn, Some("changelog"))?;
        let metadata = env.create_database::<Bytes, Bytes>(&mut wtxn, Some("metadata"))?;

        match metadata.get(&wtxn, meta_keys::SCHEMA_VERSION)? {
            None => {
                // Fresh env: seed all three metadata entries.
                metadata.put(
                    &mut wtxn,
                    meta_keys::SCHEMA_VERSION,
                    &SCHEMA_VERSION.to_be_bytes(),
                )?;
                metadata.put(&mut wtxn, meta_keys::CURRENT_VERSION, &0u64.to_be_bytes())?;
                metadata.put(&mut wtxn, meta_keys::STATE_HASH, &0u128.to_be_bytes())?;
            }
            Some(bytes) => {
                let found = decode_u16(bytes)
                    .ok_or(StorageError::CorruptMetadata("schema_version not u16"))?;
                if found != SCHEMA_VERSION {
                    // Abort the txn — we never want to commit a partial
                    // open to a mismatched env.
                    wtxn.abort();
                    return Err(StorageError::SchemaMismatch {
                        expected: SCHEMA_VERSION,
                        found,
                    });
                }
            }
        }
        wtxn.commit()?;

        Ok(Self {
            env,
            records,
            records_by_name,
            zones,
            changelog,
            metadata,
        })
    }

    /// Read the stored schema version. Used by `open()` and by migration
    /// tooling later; exposed as a method so tests can assert on it.
    #[allow(dead_code)]
    pub fn stored_schema_version(&self) -> Result<Option<u16>, StorageError> {
        let rtxn = self.env.read_txn()?;
        match self.metadata.get(&rtxn, meta_keys::SCHEMA_VERSION)? {
            None => Ok(None),
            Some(bytes) => Ok(Some(
                decode_u16(bytes).ok_or(StorageError::CorruptMetadata("schema_version not u16"))?,
            )),
        }
    }

    /// The current committed version (monotonically increasing, bumped once
    /// per write transaction regardless of how many ops it contains).
    pub fn current_version(&self) -> Result<u64, StorageError> {
        let rtxn = self.env.read_txn()?;
        read_current_version(&rtxn, self.metadata)
    }

    /// The rolling FNV1a-XOR hash of the full record set at
    /// `current_version()`. Used by drift-detection metrics that compare
    /// replicas against each other without streaming full state.
    ///
    /// This is not collision-resistant against an adversary — it's designed
    /// to detect replication bugs, bit rot, and out-of-band writes, not to
    /// defend against a forged state. A Merkle tree would be strictly
    /// stronger but ~50× the code, and we don't need those properties.
    pub fn state_hash(&self) -> Result<u128, StorageError> {
        let rtxn = self.env.read_txn()?;
        read_state_hash(&rtxn, self.metadata)
    }

    // ---------- writes ----------

    /// Insert or replace a record. Maintains the secondary index and rolls
    /// the state hash in the same atomic txn.
    ///
    /// If a record with the same UUID already exists, its old secondary
    /// index entry is removed and its contribution to the state hash is
    /// XORed out before the new one is XORed in.
    pub fn put_record(&self, record: &DnsRecord) -> Result<(), StorageError> {
        let id_bytes = record.id.as_bytes();
        let new_value = serde_json::to_vec(record)?;

        let mut wtxn = self.env.write_txn()?;

        // If a record with this id exists, pull it out so we can XOR its
        // hash out of the rolling state_hash and drop its old index entry.
        // The old row may have had a different name/type than the new one,
        // so we can't just overwrite the compound key blindly.
        let mut state = read_state_hash(&wtxn, self.metadata)?;
        if let Some(old_bytes) = self.records.get(&wtxn, id_bytes)? {
            state ^= fnv1a_128(old_bytes);
            let old_record: DnsRecord = serde_json::from_slice(old_bytes)?;
            let old_key = encode_name_key(
                &old_record.name,
                old_record.data.type_code(),
                &old_record.id,
            );
            self.records_by_name.delete(&mut wtxn, &old_key)?;
        }

        self.records.put(&mut wtxn, id_bytes, &new_value)?;
        let new_key = encode_name_key(&record.name, record.data.type_code(), &record.id);
        self.records_by_name.put(&mut wtxn, &new_key, id_bytes)?;

        state ^= fnv1a_128(&new_value);
        bump_version_and_hash(&mut wtxn, self.metadata, state)?;

        wtxn.commit()?;
        Ok(())
    }

    /// Delete a record by UUID. Returns `true` if a row was removed,
    /// `false` if the id was not present.
    pub fn delete_record_by_id(&self, id: &str) -> Result<bool, StorageError> {
        let id_bytes = id.as_bytes();
        let mut wtxn = self.env.write_txn()?;

        let old_bytes = match self.records.get(&wtxn, id_bytes)? {
            Some(b) => b.to_vec(),
            None => {
                wtxn.abort();
                return Ok(false);
            }
        };
        let old_record: DnsRecord = serde_json::from_slice(&old_bytes)?;

        self.records.delete(&mut wtxn, id_bytes)?;
        let old_key = encode_name_key(
            &old_record.name,
            old_record.data.type_code(),
            &old_record.id,
        );
        self.records_by_name.delete(&mut wtxn, &old_key)?;

        let mut state = read_state_hash(&wtxn, self.metadata)?;
        state ^= fnv1a_128(&old_bytes);
        bump_version_and_hash(&mut wtxn, self.metadata, state)?;

        wtxn.commit()?;
        Ok(true)
    }

    // ---------- reads ----------

    /// Fetch a record by its UUID.
    pub fn get_record_by_id(&self, id: &str) -> Result<Option<DnsRecord>, StorageError> {
        let rtxn = self.env.read_txn()?;
        match self.records.get(&rtxn, id.as_bytes())? {
            None => Ok(None),
            Some(bytes) => Ok(Some(serde_json::from_slice(bytes)?)),
        }
    }

    /// Fetch every record with a given (name, type). Returns an empty `Vec`
    /// if none exist. For types where multiple rows per `(name, type)` are
    /// legal (MX preference set, NS delegation set, TXT fan-out), this is
    /// the right call. A/AAAA currently enforce singleton via the write
    /// path, but that's a policy choice — the index already supports N rows.
    pub fn find_records_by_name_and_type(
        &self,
        name: &str,
        type_code: u16,
    ) -> Result<Vec<DnsRecord>, StorageError> {
        let rtxn = self.env.read_txn()?;
        let prefix = encode_name_type_prefix(name, type_code);

        let iter = self.records_by_name.prefix_iter(&rtxn, &prefix)?;
        let mut out = Vec::new();
        for entry in iter {
            let (_, id_bytes) = entry?;
            let record_bytes =
                self.records
                    .get(&rtxn, id_bytes)?
                    .ok_or(StorageError::CorruptMetadata(
                        "secondary index points to missing record",
                    ))?;
            out.push(serde_json::from_slice(record_bytes)?);
        }
        Ok(out)
    }

    /// Fetch the first record with a given (name, type), or `None` if the
    /// index is empty at that prefix. Used by the duplicate check on
    /// create/update for types where the write path enforces singleton
    /// semantics. Do **not** call this for multi-value types (NS/MX/TXT) —
    /// use [`find_records_by_name_and_type`] instead.
    pub fn find_first_by_name_and_type(
        &self,
        name: &str,
        type_code: u16,
    ) -> Result<Option<DnsRecord>, StorageError> {
        let rtxn = self.env.read_txn()?;
        let prefix = encode_name_type_prefix(name, type_code);

        let mut iter = self.records_by_name.prefix_iter(&rtxn, &prefix)?;
        match iter.next() {
            None => Ok(None),
            Some(entry) => {
                let (_, id_bytes) = entry?;
                match self.records.get(&rtxn, id_bytes)? {
                    None => Err(StorageError::CorruptMetadata(
                        "secondary index points to missing record",
                    )),
                    Some(record_bytes) => Ok(Some(serde_json::from_slice(record_bytes)?)),
                }
            }
        }
    }

    /// Fetch every record with a given name, regardless of type. The query
    /// path uses this to answer A/AAAA for the same name in one scan.
    pub fn find_records_by_name(&self, name: &str) -> Result<Vec<DnsRecord>, StorageError> {
        let rtxn = self.env.read_txn()?;
        let canon = canonical_name(name);
        let mut prefix = Vec::with_capacity(canon.len() + 1);
        prefix.extend_from_slice(canon.as_bytes());
        prefix.push(0);

        let mut out = Vec::new();
        let iter = self.records_by_name.prefix_iter(&rtxn, &prefix)?;
        for entry in iter {
            let (_, id_bytes) = entry?;
            let record_bytes =
                self.records
                    .get(&rtxn, id_bytes)?
                    .ok_or(StorageError::CorruptMetadata(
                        "secondary index points to missing record",
                    ))?;
            out.push(serde_json::from_slice(record_bytes)?);
        }
        Ok(out)
    }

    /// Return every record in the store. Used by the list endpoint and by
    /// startup metrics.
    pub fn list_all_records(&self) -> Result<Vec<DnsRecord>, StorageError> {
        let rtxn = self.env.read_txn()?;
        let iter = self.records.iter(&rtxn)?;
        let mut out = Vec::new();
        for entry in iter {
            let (_, value) = entry?;
            out.push(serde_json::from_slice(value)?);
        }
        Ok(out)
    }

    // ---------- zones ----------

    /// Insert or replace a zone. Canonicalizes the name before storing so
    /// lookups are case-insensitive. The stored `Zone.name` is the
    /// canonical form — zones are server-internal metadata, there's no
    /// display-case to preserve.
    pub fn put_zone(&self, zone: &Zone) -> Result<(), StorageError> {
        let mut canonical = zone.clone();
        canonical.name = canonical_name(&zone.name);
        let value = serde_json::to_vec(&canonical)?;
        let mut wtxn = self.env.write_txn()?;
        self.zones
            .put(&mut wtxn, canonical.name.as_bytes(), &value)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Remove a zone by name. Returns `true` if a row was removed. Does
    /// not cascade into the `records` table — orphan cleanup is the
    /// caller's responsibility (or a future sweep task).
    pub fn delete_zone(&self, name: &str) -> Result<bool, StorageError> {
        let canon = canonical_name(name);
        let mut wtxn = self.env.write_txn()?;
        let removed = self.zones.delete(&mut wtxn, canon.as_bytes())?;
        wtxn.commit()?;
        Ok(removed)
    }

    /// Fetch a zone by its (canonical) name.
    pub fn get_zone(&self, name: &str) -> Result<Option<Zone>, StorageError> {
        let canon = canonical_name(name);
        let rtxn = self.env.read_txn()?;
        match self.zones.get(&rtxn, canon.as_bytes())? {
            None => Ok(None),
            Some(bytes) => Ok(Some(serde_json::from_slice(bytes)?)),
        }
    }

    /// Return every zone this server knows about.
    pub fn list_zones(&self) -> Result<Vec<Zone>, StorageError> {
        let rtxn = self.env.read_txn()?;
        let iter = self.zones.iter(&rtxn)?;
        let mut out = Vec::new();
        for entry in iter {
            let (_, value) = entry?;
            out.push(serde_json::from_slice(value)?);
        }
        Ok(out)
    }

    /// Given any DNS name, find the most-specific zone this server is
    /// authoritative for that contains it. Returns `None` if the name is
    /// outside every registered zone — callers should answer REFUSED in
    /// that case (RFC 1035 §4.3.1).
    ///
    /// Implemented as a longest-suffix walk over labels. For a query
    /// `www.api.example.com` with zones `{example.com}` the walk probes
    /// `www.api.example.com` → `api.example.com` → `example.com` and
    /// stops on the first hit. O(labels × log(zones)) worst case, which
    /// for real DNS names means at most a handful of point lookups.
    pub fn find_zone_for(&self, name: &str) -> Result<Option<Zone>, StorageError> {
        let canon = canonical_name(name);
        let rtxn = self.env.read_txn()?;

        let mut candidate: &str = &canon;
        loop {
            if let Some(bytes) = self.zones.get(&rtxn, candidate.as_bytes())? {
                return Ok(Some(serde_json::from_slice(bytes)?));
            }
            match candidate.find('.') {
                Some(dot) => candidate = &candidate[dot + 1..],
                None => return Ok(None),
            }
        }
    }

    /// Count of records in the store. Cheaper than `list_all_records` when
    /// only the total is needed (list endpoint pagination, metrics gauge).
    pub fn record_count(&self) -> Result<u64, StorageError> {
        let rtxn = self.env.read_txn()?;
        Ok(self.records.len(&rtxn)?)
    }
}

// ---------- metadata helpers ----------

fn read_current_version(
    rtxn: &heed::RoTxn,
    metadata: Database<Bytes, Bytes>,
) -> Result<u64, StorageError> {
    match metadata.get(rtxn, meta_keys::CURRENT_VERSION)? {
        None => Err(StorageError::CorruptMetadata("current_version missing")),
        Some(bytes) => {
            decode_u64(bytes).ok_or(StorageError::CorruptMetadata("current_version not u64"))
        }
    }
}

fn read_state_hash(
    rtxn: &heed::RoTxn,
    metadata: Database<Bytes, Bytes>,
) -> Result<u128, StorageError> {
    match metadata.get(rtxn, meta_keys::STATE_HASH)? {
        None => Err(StorageError::CorruptMetadata("state_hash missing")),
        Some(bytes) => {
            decode_u128(bytes).ok_or(StorageError::CorruptMetadata("state_hash not u128"))
        }
    }
}

fn bump_version_and_hash(
    wtxn: &mut RwTxn,
    metadata: Database<Bytes, Bytes>,
    new_state_hash: u128,
) -> Result<u64, StorageError> {
    // Read inside the same write txn so concurrent readers can't make us
    // skip a version. Writers are serialised by LMDB already.
    let current = match metadata.get(wtxn, meta_keys::CURRENT_VERSION)? {
        Some(b) => decode_u64(b).ok_or(StorageError::CorruptMetadata("current_version not u64"))?,
        None => return Err(StorageError::CorruptMetadata("current_version missing")),
    };
    let next = current + 1;
    metadata.put(wtxn, meta_keys::CURRENT_VERSION, &next.to_be_bytes())?;
    metadata.put(wtxn, meta_keys::STATE_HASH, &new_state_hash.to_be_bytes())?;
    Ok(next)
}

fn decode_u16(bytes: &[u8]) -> Option<u16> {
    bytes.try_into().ok().map(u16::from_be_bytes)
}
fn decode_u64(bytes: &[u8]) -> Option<u64> {
    bytes.try_into().ok().map(u64::from_be_bytes)
}
fn decode_u128(bytes: &[u8]) -> Option<u128> {
    bytes.try_into().ok().map(u128::from_be_bytes)
}

/// FNV-1a 128-bit hash. Stable across runs and machines, no deps.
///
/// Used as the per-record contribution to the rolling state hash. The
/// rolling hash is XOR-combined, so we only need a stable byte→u128 mixer
/// with decent diffusion. FNV-1a is the smallest such thing that isn't
/// obviously broken — see the `state_hash` docs for the threat model.
fn fnv1a_128(bytes: &[u8]) -> u128 {
    const OFFSET: u128 = 0x6c62272e07bb014262b821756295c58d;
    const PRIME: u128 = 0x0000000001000000000000000000013b;
    let mut hash = OFFSET;
    for b in bytes {
        hash ^= *b as u128;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Resolve the LMDB map size from the environment, falling back to
/// [`DEFAULT_MAP_SIZE_BYTES`]. Invalid values in the env var are a startup
/// error — we won't silently fall back, because "my config was ignored" is
/// the worst k8s bug.
fn resolve_map_size() -> Result<usize, StorageError> {
    match std::env::var(MAP_SIZE_ENV) {
        Ok(s) => s
            .parse::<usize>()
            .map_err(|_| StorageError::CorruptMetadata("RIND_LMDB_MAP_SIZE must be a byte count")),
        Err(_) => Ok(DEFAULT_MAP_SIZE_BYTES),
    }
}

/// Canonicalize a DNS name per RFC 4343: lowercase (ASCII only, per
/// §2.1 — non-ASCII never appears in wire-format DNS names) and strip any
/// trailing `.` so `example.com.` and `example.com` hash to the same slot.
///
/// All index writes and lookups route through this so `EXAMPLE.com` and
/// `example.com.` are the same key. The stored `DnsRecord.name` keeps its
/// original case (case-preserving, case-insensitive — the compliant shape).
pub fn canonical_name(name: &str) -> String {
    name.trim_end_matches('.').to_ascii_lowercase()
}

/// Encode a prefix of [`encode_name_key`] up through the trailing NUL after
/// `type_be`. Scanning with this prefix returns every record for a given
/// `(name, type)` regardless of uuid, in uuid-sorted order.
pub(crate) fn encode_name_type_prefix(name: &str, type_code: u16) -> Vec<u8> {
    let canon = canonical_name(name);
    let mut prefix = Vec::with_capacity(canon.len() + 1 + 2 + 1);
    prefix.extend_from_slice(canon.as_bytes());
    prefix.push(0);
    prefix.extend_from_slice(&type_code.to_be_bytes());
    prefix.push(0);
    prefix
}

/// Encode the compound key for `records_by_name`: `name \0 type_be \0 uuid`.
///
/// The NUL separators mean a prefix scan on `name\0` returns every record
/// for that name in one contiguous range, and within that range the
/// type-code ordering groups A before AAAA. Hand-rolled rather than using a
/// delimiter-encoding scheme because UUIDs and DNS names never contain NUL
/// bytes, so there's nothing to escape.
pub(crate) fn encode_name_key(name: &str, type_code: u16, record_id: &str) -> Vec<u8> {
    let canon = canonical_name(name);
    let mut key = Vec::with_capacity(canon.len() + 1 + 2 + 1 + record_id.len());
    key.extend_from_slice(canon.as_bytes());
    key.push(0);
    key.extend_from_slice(&type_code.to_be_bytes());
    key.push(0);
    key.extend_from_slice(record_id.as_bytes());
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::RecordData;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use tempfile::TempDir;

    fn tmpstore() -> (TempDir, LmdbStore) {
        let dir = TempDir::new().unwrap();
        let store = LmdbStore::open(dir.path()).unwrap();
        (dir, store)
    }

    fn rec(name: &str, data: RecordData) -> DnsRecord {
        let now = Utc::now();
        DnsRecord {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            ttl: 300,
            class: "IN".to_string(),
            data,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn open_seeds_metadata_on_fresh_env() {
        let (_dir, store) = tmpstore();
        assert_eq!(store.stored_schema_version().unwrap(), Some(SCHEMA_VERSION));
        assert_eq!(store.current_version().unwrap(), 0);
        assert_eq!(store.state_hash().unwrap(), 0);
    }

    #[test]
    fn reopen_preserves_metadata() {
        let dir = TempDir::new().unwrap();
        {
            let store = LmdbStore::open(dir.path()).unwrap();
            store
                .put_record(&rec(
                    "a.example.com",
                    RecordData::A {
                        ip: Ipv4Addr::new(1, 2, 3, 4),
                    },
                ))
                .unwrap();
        }
        let store = LmdbStore::open(dir.path()).unwrap();
        assert_eq!(store.current_version().unwrap(), 1);
        assert_eq!(store.list_all_records().unwrap().len(), 1);
    }

    #[test]
    fn put_then_get_roundtrips() {
        let (_dir, store) = tmpstore();
        let r = rec(
            "a.example.com",
            RecordData::A {
                ip: Ipv4Addr::new(1, 2, 3, 4),
            },
        );
        store.put_record(&r).unwrap();
        let fetched = store.get_record_by_id(&r.id).unwrap().unwrap();
        assert_eq!(fetched, r);
    }

    #[test]
    fn get_by_name_and_type_finds_the_right_row() {
        let (_dir, store) = tmpstore();
        let a = rec(
            "dual.example.com",
            RecordData::A {
                ip: Ipv4Addr::new(1, 2, 3, 4),
            },
        );
        let aaaa = rec(
            "dual.example.com",
            RecordData::Aaaa {
                ip: Ipv6Addr::LOCALHOST,
            },
        );
        store.put_record(&a).unwrap();
        store.put_record(&aaaa).unwrap();

        let got_a = store
            .find_first_by_name_and_type("dual.example.com", 1)
            .unwrap()
            .unwrap();
        assert_eq!(got_a.id, a.id);
        let got_v6 = store
            .find_first_by_name_and_type("dual.example.com", 28)
            .unwrap()
            .unwrap();
        assert_eq!(got_v6.id, aaaa.id);
    }

    #[test]
    fn find_records_by_name_returns_all_types() {
        let (_dir, store) = tmpstore();
        store
            .put_record(&rec(
                "dual.example.com",
                RecordData::A {
                    ip: Ipv4Addr::new(1, 2, 3, 4),
                },
            ))
            .unwrap();
        store
            .put_record(&rec(
                "dual.example.com",
                RecordData::Aaaa {
                    ip: Ipv6Addr::LOCALHOST,
                },
            ))
            .unwrap();
        // Different name — must NOT be returned.
        store
            .put_record(&rec(
                "other.example.com",
                RecordData::A {
                    ip: Ipv4Addr::new(5, 6, 7, 8),
                },
            ))
            .unwrap();

        let hits = store.find_records_by_name("dual.example.com").unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn find_records_by_name_and_type_returns_all_matches() {
        // Bypass the singleton-style write path by constructing the index
        // entries directly — exercises the multi-value read path where two
        // rows legitimately share (name, type).
        let (_dir, store) = tmpstore();
        let r1 = rec(
            "multi.example.com",
            RecordData::A {
                ip: Ipv4Addr::new(1, 2, 3, 4),
            },
        );
        let r2 = rec(
            "multi.example.com",
            RecordData::A {
                ip: Ipv4Addr::new(5, 6, 7, 8),
            },
        );
        store.put_record(&r1).unwrap();
        store.put_record(&r2).unwrap();

        let hits = store
            .find_records_by_name_and_type("multi.example.com", 1)
            .unwrap();
        assert_eq!(hits.len(), 2);

        let none = store
            .find_records_by_name_and_type("multi.example.com", 28)
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn delete_removes_record_and_index() {
        let (_dir, store) = tmpstore();
        let r = rec(
            "gone.example.com",
            RecordData::A {
                ip: Ipv4Addr::new(1, 2, 3, 4),
            },
        );
        store.put_record(&r).unwrap();
        assert!(store.delete_record_by_id(&r.id).unwrap());
        assert!(store.get_record_by_id(&r.id).unwrap().is_none());
        assert!(store
            .find_first_by_name_and_type("gone.example.com", 1)
            .unwrap()
            .is_none());
        assert!(!store.delete_record_by_id(&r.id).unwrap());
    }

    #[test]
    fn put_replacing_existing_swaps_secondary_index() {
        let (_dir, store) = tmpstore();
        let mut r = rec(
            "swap.example.com",
            RecordData::A {
                ip: Ipv4Addr::new(1, 2, 3, 4),
            },
        );
        store.put_record(&r).unwrap();
        r.name = "renamed.example.com".to_string();
        store.put_record(&r).unwrap();

        assert!(store
            .find_first_by_name_and_type("swap.example.com", 1)
            .unwrap()
            .is_none());
        assert_eq!(
            store
                .find_first_by_name_and_type("renamed.example.com", 1)
                .unwrap()
                .unwrap()
                .id,
            r.id
        );
    }

    #[test]
    fn version_and_state_hash_update_per_write() {
        let (_dir, store) = tmpstore();
        assert_eq!(store.current_version().unwrap(), 0);
        assert_eq!(store.state_hash().unwrap(), 0);

        let r = rec(
            "h.example.com",
            RecordData::A {
                ip: Ipv4Addr::new(1, 2, 3, 4),
            },
        );
        store.put_record(&r).unwrap();
        assert_eq!(store.current_version().unwrap(), 1);
        let h1 = store.state_hash().unwrap();
        assert_ne!(h1, 0);

        store.delete_record_by_id(&r.id).unwrap();
        assert_eq!(store.current_version().unwrap(), 2);
        // Delete must XOR the same contribution back out — empty store
        // returns to a zero hash.
        assert_eq!(store.state_hash().unwrap(), 0);
    }

    fn test_zone(name: &str) -> Zone {
        Zone {
            name: name.to_string(),
            mname: format!("ns1.{}", name),
            rname: format!("admin.{}", name),
            serial: 1,
            refresh: 3600,
            retry: 600,
            expire: 604800,
            minimum: 300,
        }
    }

    #[test]
    fn canonical_name_lowercases_and_strips_trailing_dot() {
        assert_eq!(canonical_name("EXAMPLE.com."), "example.com");
        assert_eq!(canonical_name("example.com"), "example.com");
        assert_eq!(canonical_name("Foo.Bar.Example.COM"), "foo.bar.example.com");
    }

    #[test]
    fn case_and_trailing_dot_do_not_fragment_the_index() {
        let (_dir, store) = tmpstore();
        store
            .put_record(&rec(
                "EXAMPLE.com",
                RecordData::A {
                    ip: Ipv4Addr::new(1, 2, 3, 4),
                },
            ))
            .unwrap();

        // Query with different case and a trailing dot — must still hit.
        let hit = store
            .find_first_by_name_and_type("example.com.", 1)
            .unwrap();
        assert!(hit.is_some());
        let hits = store.find_records_by_name("Example.COM").unwrap();
        assert_eq!(hits.len(), 1);
        // Stored record preserves original case in the value.
        assert_eq!(hits[0].name, "EXAMPLE.com");
    }

    #[test]
    fn zone_crud_roundtrips() {
        let (_dir, store) = tmpstore();
        let z = test_zone("example.com");
        store.put_zone(&z).unwrap();
        assert_eq!(store.get_zone("example.com").unwrap().unwrap(), z);
        // Case-insensitive lookup on zones too.
        assert_eq!(store.get_zone("EXAMPLE.COM.").unwrap().unwrap(), z);
        assert_eq!(store.list_zones().unwrap().len(), 1);
        assert!(store.delete_zone("example.com").unwrap());
        assert!(store.get_zone("example.com").unwrap().is_none());
        assert!(!store.delete_zone("example.com").unwrap());
    }

    #[test]
    fn find_zone_for_picks_longest_suffix_match() {
        let (_dir, store) = tmpstore();
        store.put_zone(&test_zone("example.com")).unwrap();
        store.put_zone(&test_zone("sub.example.com")).unwrap();

        // www.sub.example.com → sub.example.com (more specific wins)
        let hit = store.find_zone_for("www.sub.example.com").unwrap().unwrap();
        assert_eq!(hit.name, "sub.example.com");

        // api.example.com → example.com (the only containing zone)
        let hit = store.find_zone_for("api.example.com").unwrap().unwrap();
        assert_eq!(hit.name, "example.com");

        // Exact apex match.
        let hit = store.find_zone_for("example.com").unwrap().unwrap();
        assert_eq!(hit.name, "example.com");

        // Out of zone → None (caller answers REFUSED).
        assert!(store.find_zone_for("other.net").unwrap().is_none());
    }

    #[test]
    fn schema_mismatch_fails_open() {
        let dir = TempDir::new().unwrap();
        {
            let _store = LmdbStore::open(dir.path()).unwrap();
        }
        // Poke a bogus schema version directly.
        {
            let env = unsafe {
                EnvOpenOptions::new()
                    .max_dbs(MAX_DBS)
                    .map_size(DEFAULT_MAP_SIZE_BYTES)
                    .open(dir.path())
                    .unwrap()
            };
            let mut wtxn = env.write_txn().unwrap();
            let metadata: Database<Bytes, Bytes> =
                env.open_database(&wtxn, Some("metadata")).unwrap().unwrap();
            metadata
                .put(&mut wtxn, meta_keys::SCHEMA_VERSION, &999u16.to_be_bytes())
                .unwrap();
            wtxn.commit().unwrap();
        }
        let result = LmdbStore::open(dir.path());
        match result {
            Err(StorageError::SchemaMismatch { expected, found }) => {
                assert_eq!(expected, SCHEMA_VERSION);
                assert_eq!(found, 999);
            }
            Err(e) => panic!("expected SchemaMismatch, got error {:?}", e),
            Ok(_) => panic!("expected SchemaMismatch, got Ok"),
        }
    }
}
