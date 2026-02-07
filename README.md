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
| **enigma-keys** | `KeyProvider` trait + local hybrid post-quantum (Argon2id + ML-KEM-768), Azure Key Vault, GCP Secret Manager, AWS Secrets Manager |
| **enigma-cli** | CLI binary (`enigma`) — init, backup, restore, verify, list, status, config, gc, encrypt-cred |
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
- **Vault key providers** — Azure Key Vault, GCP Secret Manager, AWS Secrets Manager (behind feature flags)
- **TLS S3 gateway** — optional HTTPS with rustls (PEM cert/key)
- **Prometheus metrics** — `/metrics` endpoint on configurable port (behind `metrics` feature)
- **Encrypted credentials** — AES-256-GCM encrypted secrets in TOML config (`enc:` prefix)
- **Garbage collection** — `enigma gc` to find and delete orphaned chunks (with `--dry-run`)
- **Selective restore** — `--path`, `--glob`, `--list` filters on restore
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

Enigma supports multiple key provider backends. Set `key_provider` in config:

| Provider | `key_provider` | Required config | Feature flag |
|----------|---------------|-----------------|-------------|
| Local (default) | `"local"` | `keyfile_path` + passphrase | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

Cloud credentials in config can be encrypted with `enigma encrypt-cred <value>` — produces an `enc:...` token to paste in TOML.

Additional security:
- File permissions on `enigma.toml`
- Environment variables (`ENIGMA_PASSPHRASE`, AWS env vars, etc.)
- The keyfile itself is encrypted with the passphrase

## Quick Start

### Build

```bash
# Prerequisites: Rust 1.85+, protoc (for tonic/prost)
cargo build --release --workspace

# With optional features
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

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

# Restore (full)
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# Selective restore
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # prefix filter
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # glob filter
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # list files only

# Garbage collection
enigma gc --dry-run    # list orphaned chunks
enigma gc              # delete orphaned chunks

# Encrypt a credential for config
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

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
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # for azure-keyvault
# gcp_project_id = "my-project"                     # for gcp-secretmanager
# aws_region = "us-east-1"                          # for aws-secretsmanager
# secret_prefix = "enigma-key"                      # prefix for vault secret names

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
# tls_cert = "/path/to/cert.pem"         # enables HTTPS (feature: tls)
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # Prometheus endpoint (feature: metrics)

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
| `AWS_REGION` | AWS region for Secrets Manager key provider |
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

### Unit & Integration Tests (49+ tests)

```
cargo test --workspace

enigma-core .......... 36 tests (chunking, crypto, compression, config, credentials, dedup, distributor, manifest, types)
enigma-core (bench) ..  8 tests (SHA-256, AES-GCM, zstd, CDC, fixed, full pipeline throughput)
enigma-keys ..........  5 tests (ML-KEM keypair, hybrid derivation, rotation, wrong passphrase)
enigma-keys (bench) ..  1 test  (Argon2id + ML-KEM-768 key derivation timing)
enigma-storage .......  4 tests (local + S3 provider tests)
enigma-keys (vault) ..  4 tests (Azure KV, GCP SM — behind features + real credentials)
enigma-keys (aws) ....  2 tests (AWS SM — behind feature + real credentials)
                       ──
                       49+ unit + 9 bench, 0 failures
```

### Vault tests (require real credentials)

```bash
# Azure Key Vault
AZURE_KEYVAULT_URL="https://my-vault.vault.azure.net/" \
  cargo test -p enigma-keys --features azure-keyvault --test vault_providers

# GCP Secret Manager
GCP_PROJECT_ID=my-project \
  cargo test -p enigma-keys --features gcp-secretmanager --test vault_providers

# AWS Secrets Manager
AWS_REGION=us-east-1 \
  cargo test -p enigma-keys --features aws-secretsmanager --test vault_providers
```

### Test Coverage

| Module | What's tested |
|--------|--------------|
| `chunk::cdc` | Empty file, small file (single chunk), large file (multi-chunk), deterministic hashes |
| `chunk::fixed` | Empty file, exact multiple, remainder handling |
| `compression` | Roundtrip compress/decompress, empty data |
| `config` | TOML roundtrip serialization, missing file error |
| `config::credentials` | Encrypt/decrypt roundtrip, plaintext passthrough |
| `crypto` | Encrypt/decrypt roundtrip (raw + chunk), wrong key rejection, wrong AAD rejection, unique nonces |
| `dedup` | Deterministic hashing, different data → different hashes, duplicate detection |
| `distributor` | Round-robin cycling, weighted distribution, provider lookup |
| `manifest::schema` | Table creation, migration idempotency |
| `manifest::queries` | Full backup flow, list ordering, chunk dedup ref counting, logs |
| `types` | ChunkHash hex roundtrip, storage key format, KeyMaterial zeroize, ProviderType parsing |
| `keys::local` | Create/open keyfile, wrong passphrase, ML-KEM sizes, hybrid key independence, rotation |
| `keys::vault` | Azure KV, GCP SM, AWS SM — create, get, rotate, list (integration) |
| `storage::local` | Connection test, upload/download roundtrip, manifest roundtrip |

### Performance (Apple M3 Pro, release build)

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### Pipeline Throughput

| Stage | 1 MB | 4 MB | 16 MB |
|-------|------|------|-------|
| SHA-256 hashing | 340 MB/s | 318 MB/s | 339 MB/s |
| AES-256-GCM encrypt | 135 MB/s | 135 MB/s | 137 MB/s |
| AES-256-GCM decrypt | 137 MB/s | 135 MB/s | 137 MB/s |
| zstd compress (random) | 4224 MB/s | 2484 MB/s | 1830 MB/s |
| zstd compress (text) | 6762 MB/s | 6242 MB/s | — |

#### Chunking

| Engine | 4 MB file | 16 MB file | 64 MB file |
|--------|-----------|------------|------------|
| CDC (4 MB target) | 271 MB/s | 227 MB/s | 266 MB/s |
| Fixed (4 MB) | 308 MB/s | 221 MB/s | 310 MB/s |

#### Full Pipeline (Chunk → Hash → Compress → Encrypt)

| Input | Chunks | Throughput |
|-------|--------|-----------|
| 4 MB | 1 | 70 MB/s |
| 16 MB | 2-3 | 66 MB/s |
| 64 MB | 10-16 | 69 MB/s |

#### Key Derivation (Argon2id + ML-KEM-768 + HKDF)

| Operation | Time |
|-----------|------|
| Create (keygen + encrypt) | 17 ms |
| Open (decrypt + derive) | 15 ms |

> Bottleneck is AES-256-GCM (~135 MB/s). SHA-256 and zstd are much faster.
> Network I/O to cloud backends is typically the real bottleneck in production.

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

- [x] Vault integration for secrets (AWS Secrets Manager, Azure Key Vault, GCP Secret Manager)
- [x] Prometheus metrics endpoint
- [x] TLS support for S3 gateway
- [x] Encrypted credentials in config
- [x] Garbage collection for orphaned chunks
- [x] Selective restore (path/glob filters)
- [ ] Incremental backups (only changed files)
- [ ] Bandwidth throttling
- [ ] Web UI dashboard
- [ ] Snapshot-based Raft recovery
- [ ] Erasure coding (Reed-Solomon) as alternative to replication

## License

MIT
