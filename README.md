# Enigma

Multi-cloud encrypted backup tool with an S3-compatible gateway and Raft-based high availability.

Enigma encrypts, chunks, deduplicates, optionally compresses, and distributes data across multiple cloud storage backends. It exposes an S3-compatible API so any S3 client (aws-cli, mc, rclone, SDKs) can interact with it transparently.

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## Architecture

```
                         S3 Clients (aws-cli, mc, rclone, SDKs)
                                       |
                                       v
                     +-----------------+------------------+
                     |          enigma-proxy              |
                     |   S3 Gateway (s3s) + Raft (opt.)   |
                     +---------+-----------+--------------+
                               |           |
                  +------------+           +------------+
                  |  Data Path                Metadata   |
                  |  (chunks)                 (Raft)     |
                  v                                      v
    +-------------+-------------+         +----+--------+----+
    |             |             |         | Node 1 | Node 2 | Node 3 |
    v             v             v         +--------+---------+--------+
 +------+   +--------+   +--------+              |
 | S3 / |   | Azure  |   |  GCS   |    SQLite Manifest (replicated)
 | MinIO|   |  Blob  |   | Bucket |
 +------+   +--------+   +--------+

             enigma-cli (backup / restore / verify)
                  |
                  +---> Same pipeline: Chunk -> Hash -> [Compress] -> Encrypt -> Upload
```

### Data Pipeline

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

The hash is always computed on the **original plaintext**, so deduplication works identically whether compression is enabled or not. The `size_compressed` column in the manifest (NULL = not compressed) tells the read path whether decompression is needed — fully backward compatible.

### Crates

| Crate | Role |
|-------|------|
| **enigma-core** | Chunking (FastCDC / Fixed), crypto (AES-256-GCM), dedup (SHA-256), compression (zstd), distributor, manifest (SQLite), config (TOML) |
| **enigma-storage** | `StorageProvider` trait + implementations: Local, S3, S3-compatible, Azure Blob, GCS |
| **enigma-keys** | `KeyProvider` trait + local hybrid post-quantum implementation (Argon2id + ML-KEM-768) |
| **enigma-cli** | CLI binary (`enigma`) — init, backup, restore, verify, list, status, config |
| **enigma-s3** | S3 frontend built on s3s v0.11 — PutObject, GetObject, HeadObject, DeleteObject, ListObjectsV2, buckets, multipart |
| **enigma-raft** | Raft consensus (openraft v0.9 + tonic gRPC) — state machine wrapping ManifestDb for HA metadata replication |
| **enigma-proxy** | Binary combining S3 gateway + Raft — single-node or cluster mode |

## Features

- **End-to-end encryption** — AES-256-GCM, keys never leave the client
- **Hybrid post-quantum key derivation** — Argon2id + ML-KEM-768 (FIPS 203) combined via HKDF-SHA256
- **Content-defined chunking** — FastCDC with configurable target size (default 4 MB) or fixed-size chunks
- **SHA-256 deduplication** — identical chunks stored only once across all backups
- **Optional zstd compression** — applied before encryption, disabled by default, backward compatible
- **Multi-cloud distribution** — round-robin or weighted distribution across providers
- **S3-compatible gateway** — full CRUD, multipart uploads, ListObjectsV2 with prefix/delimiter
- **Raft HA** — 3-node consensus for metadata replication (data goes direct to backends)
- **Single-node mode** — works without Raft, local storage fallback if no providers configured
- **Audit trail** — SQLite manifest with backup logs and chunk reference counting
- **Key rotation** — generate new hybrid keys, old keys remain accessible by ID

## Security Model

### Key Derivation

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id**: memory-hard, resistant to GPU/ASIC attacks
- **ML-KEM-768**: NIST FIPS 203 post-quantum KEM — protects against future quantum computers
- **HKDF**: combines both sources; security holds if **either** source is unbroken
- **Keystore on disk**: `[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **Zeroization**: all key material is zeroized on drop (`zeroize` crate)

### Encryption

- **AES-256-GCM** per chunk with random 12-byte nonce
- **AAD** (Additional Authenticated Data): chunk SHA-256 hash — binds ciphertext to its content identity
- Encrypted data is stored; nonce is stored in the manifest

### Secrets Management

Currently, cloud credentials are stored in the TOML config file or passed via environment variables. There is **no vault integration yet** — the architecture is ready for it (`key_provider` field supports future `"vault"`, `"aws-secrets"`, `"azure-keyvault"`, `"gcp-secretmanager"` backends), but for now secrets management relies on:

- File permissions on `enigma.toml`
- Environment variables (`ENIGMA_PASSPHRASE`, AWS env vars, etc.)
- The keyfile itself is encrypted with the passphrase

## Quick Start

### Build

```bash
# Prerequisites: Rust 1.85+, protoc (for tonic/prost)
cargo build --release --workspace

# Binary locations
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # S3 gateway
```

### CLI Usage

```bash
# Initialize (creates config + encrypted keyfile)
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# Backup a directory
enigma --passphrase "my-secret" backup /path/to/data

# List backups
enigma list

# Verify integrity
enigma --passphrase "my-secret" verify <backup-id>

# Restore
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# Show status / config
enigma status
enigma config
```

### S3 Gateway (Single Node)

```bash
# Start the proxy
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# Use any S3 client
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## Configuration

### Full Reference (`enigma.toml`)

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" (only option for now)
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"

# Chunking — pick one:
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 MB (default)

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 MB

# Compression (optional, disabled by default)
[enigma.compression]
enabled = false                          # set to true to enable zstd
level = 3                                # zstd level 1-22 (default: 3)

# S3 proxy (enigma-proxy only)
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"

# Storage providers — add as many as needed
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # Also accepts: "minio", "rustfs", "garage"
bucket = "enigma-chunks"
region = "us-east-1"
endpoint_url = "http://127.0.0.1:9000"
path_style = true
access_key = "minioadmin"
secret_key = "minioadmin"
weight = 1

[[providers]]
name = "azure-backup"
type = "Azure"
bucket = "enigma-container"              # Container name
region = "westeurope"
weight = 1

[[providers]]
name = "gcs-archive"
type = "Gcs"
bucket = "my-enigma-gcs-bucket"
region = "europe-west1"
weight = 1

[[providers]]
name = "local-fallback"
type = "Local"
bucket = "/data/enigma-local"            # Local directory path
weight = 1

# Raft (optional, for multi-node HA)
[raft]
node_id = 1
data_dir = "/data/raft"
grpc_addr = "0.0.0.0:9000"
election_timeout_ms = 1000
heartbeat_interval_ms = 300
snapshot_threshold = 10000

[[raft.peers]]
id = 1
addr = "enigma-0.enigma:9000"

[[raft.peers]]
id = 2
addr = "enigma-1.enigma:9000"

[[raft.peers]]
id = 3
addr = "enigma-2.enigma:9000"
```

### Provider Types

| Type | Value(s) | Notes |
|------|----------|-------|
| Local filesystem | `Local` | `bucket` = directory path |
| AWS S3 | `S3` | Uses AWS SDK default credential chain |
| S3-compatible | `S3Compatible`, `minio`, `rustfs`, `garage` | Requires `endpoint_url`, `path_style = true` |
| Azure Blob Storage | `Azure` | `bucket` = container name |
| Google Cloud Storage | `Gcs` | Uses Application Default Credentials |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ENIGMA_PASSPHRASE` | Passphrase for key encryption (avoids interactive prompt) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | AWS credentials (for S3 provider) |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | Azure credentials |
| `GOOGLE_APPLICATION_CREDENTIALS` | Path to GCP service account JSON |
| `RUST_LOG` | Log level filter (e.g., `enigma=info,tower=warn`) |

## S3 API Compatibility

| Operation | Supported |
|-----------|-----------|
| CreateBucket | Yes |
| DeleteBucket | Yes (must be empty) |
| HeadBucket | Yes |
| ListBuckets | Yes |
| PutObject | Yes |
| GetObject | Yes |
| HeadObject | Yes |
| DeleteObject | Yes |
| ListObjectsV2 | Yes (prefix, delimiter, max-keys, continuation-token) |
| CreateMultipartUpload | Yes |
| UploadPart | Yes |
| CompleteMultipartUpload | Yes |
| AbortMultipartUpload | Yes |

## Tests

### Unit Tests (42 tests)

```
cargo test --workspace

enigma-core .......... 34 tests (chunking, crypto, compression, config, dedup, distributor, manifest, types)
enigma-keys ..........  5 tests (ML-KEM keypair, hybrid derivation, rotation, wrong passphrase)
enigma-storage .......  3 tests (local provider: upload, download, roundtrip)
                       ──
                       42 passed, 0 failed
```

### Test Coverage

| Module | What's tested |
|--------|--------------|
| `chunk::cdc` | Empty file, small file (single chunk), large file (multi-chunk), deterministic hashes |
| `chunk::fixed` | Empty file, exact multiple, remainder handling |
| `compression` | Roundtrip compress/decompress, empty data |
| `config` | TOML roundtrip serialization, missing file error |
| `crypto` | Encrypt/decrypt roundtrip (raw + chunk), wrong key rejection, wrong AAD rejection, unique nonces |
| `dedup` | Deterministic hashing, different data → different hashes, duplicate detection |
| `distributor` | Round-robin cycling, weighted distribution, provider lookup |
| `manifest::schema` | Table creation, migration idempotency |
| `manifest::queries` | Full backup flow, list ordering, chunk dedup ref counting, logs |
| `types` | ChunkHash hex roundtrip, storage key format, KeyMaterial zeroize, ProviderType parsing |
| `keys::local` | Create/open keyfile, wrong passphrase, ML-KEM sizes, hybrid key independence, rotation |
| `storage::local` | Connection test, upload/download roundtrip, manifest roundtrip |

### E2E Test

```bash
# Requires 3 RustFS instances running (Kind cluster or docker-compose)
./tests/e2e_rustfs.sh
```

Tests: init → backup 5 files → verify → restore → diff original vs restored.

### CI Pipeline

GitHub Actions runs on every push/PR to `main`:
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## Deployment

### Docker Compose (3-node cluster)

```bash
docker compose up -d
# 3 enigma-proxy nodes (ports 8333-8335) + 3 RustFS backends (ports 19001-19003)

# Test
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes (StatefulSet)

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3 enigma pods (StatefulSet) + 3 RustFS deployments
# S3 access via enigma-s3 ClusterIP service on port 8333
```

### Single Binary

```bash
# CLI mode (backup/restore)
enigma --config-dir /etc/enigma backup /data

# Gateway mode (S3 proxy)
enigma-proxy --config /etc/enigma/config.toml
```

## Roadmap

- [ ] Vault integration for secrets (AWS Secrets Manager, Azure Key Vault, GCP Secret Manager)
- [ ] Incremental backups (only changed files)
- [ ] Bandwidth throttling
- [ ] Web UI dashboard
- [ ] Prometheus metrics endpoint
- [ ] Snapshot-based Raft recovery
- [ ] Erasure coding (Reed-Solomon) as alternative to replication

## License

MIT
