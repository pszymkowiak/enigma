[English](README.md) | [Français](README_fr.md) | [Español](README_es.md) | [Deutsch](README_de.md) | [Italiano](README_it.md) | [Português](README_pt.md) | **Nederlands** | [Polski](README_pl.md) | [Русский](README_ru.md) | [日本語](README_ja.md) | [中文](README_zh.md) | [العربية](README_ar.md) | [한국어](README_ko.md)

# Enigma

Multi-cloud versleutelde back-uptool met S3-compatibele gateway en Raft-gebaseerde hoge beschikbaarheid.

Enigma versleutelt, fragmenteert, dedupliceert, comprimeert optioneel en distribueert gegevens over meerdere cloud-opslagbackends. Het biedt een S3-compatibele API zodat elke S3-client (aws-cli, mc, rclone, SDKs) er transparant mee kan communiceren.

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## Architectuur

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

### Datapipeline

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

De hash wordt altijd berekend op de **originele platte tekst**, zodat deduplicatie identiek werkt ongeacht of compressie is ingeschakeld. De kolom `size_compressed` in het manifest (NULL = niet gecomprimeerd) geeft het leespad aan of decompressie nodig is — volledig achterwaarts compatibel.

### Crates

| Crate | Rol |
|-------|-----|
| **enigma-core** | Fragmentatie (FastCDC / Fixed), crypto (AES-256-GCM), dedup (SHA-256), compressie (zstd), distributeur, manifest (SQLite), configuratie (TOML) |
| **enigma-storage** | Trait `StorageProvider` + implementaties: Local, S3, S3-compatibel, Azure Blob, GCS |
| **enigma-keys** | Trait `KeyProvider` + lokaal hybride post-quantum (Argon2id + ML-KEM-768), Azure Key Vault, GCP Secret Manager, AWS Secrets Manager |
| **enigma-cli** | CLI-binary (`enigma`) — init, backup, restore, verify, list, status, config, gc, encrypt-cred |
| **enigma-s3** | S3-frontend gebouwd op s3s v0.11 — PutObject, GetObject, HeadObject, DeleteObject, ListObjectsV2, buckets, multipart |
| **enigma-raft** | Raft-consensus (openraft v0.9 + tonic gRPC) — state machine die ManifestDb omhult voor HA-metadatareplicatie |
| **enigma-proxy** | Binary die S3-gateway + Raft combineert — enkelvoudige node- of clustermodus |

## Functies

- **End-to-end versleuteling** — AES-256-GCM, sleutels verlaten nooit de client
- **Hybride post-quantum sleutelafleiding** — Argon2id + ML-KEM-768 (FIPS 203) gecombineerd via HKDF-SHA256
- **Content-gedefinieerde fragmentatie** — FastCDC met configureerbare doelgrootte (standaard 4 MB) of fragmenten met vaste grootte
- **SHA-256 deduplicatie** — identieke fragmenten worden slechts eenmaal opgeslagen over alle back-ups
- **Optionele zstd-compressie** — toegepast voor versleuteling, standaard uitgeschakeld, achterwaarts compatibel
- **Multi-cloud distributie** — round-robin of gewogen distributie over providers
- **S3-compatibele gateway** — volledige CRUD, multipart uploads, ListObjectsV2 met prefix/delimiter
- **Raft HA** — 3-node consensus voor metadatareplicatie (gegevens gaan direct naar backends)
- **Enkelvoudige node-modus** — werkt zonder Raft, lokale opslag-fallback als geen providers geconfigureerd
- **Vault-sleutelproviders** — Azure Key Vault, GCP Secret Manager, AWS Secrets Manager (achter feature flags)
- **TLS S3-gateway** — optioneel HTTPS met rustls (PEM cert/sleutel)
- **Prometheus-metrics** — `/metrics`-endpoint op configureerbare poort (achter feature `metrics`)
- **Versleutelde inloggegevens** — AES-256-GCM versleutelde geheimen in TOML-configuratie (prefix `enc:`)
- **Garbage collection** — `enigma gc` om verweesde fragmenten te vinden en te verwijderen (met `--dry-run`)
- **Selectief herstel** — `--path`, `--glob`, `--list`-filters bij herstel
- **Auditspoor** — SQLite-manifest met back-uplogs en fragmentreferentietelling
- **Sleutelrotatie** — nieuwe hybride sleutels genereren, oude sleutels blijven toegankelijk via ID

## Beveiligingsmodel

### Sleutelafleiding

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id**: geheugenresistent, bestand tegen GPU/ASIC-aanvallen
- **ML-KEM-768**: NIST FIPS 203 post-quantum KEM — beschermt tegen toekomstige kwantumcomputers
- **HKDF**: combineert beide bronnen; beveiliging blijft gewaarborgd als **een van beide** bronnen niet gecompromitteerd is
- **Keystore op schijf**: `[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **Nulstelling**: al het sleutelmateriaal wordt genulsteld bij vernietiging (crate `zeroize`)

### Versleuteling

- **AES-256-GCM** per fragment met willekeurige 12-byte nonce
- **AAD** (Additional Authenticated Data): SHA-256-hash van het fragment — bindt de cijfertekst aan zijn inhoudsidentiteit
- Versleutelde gegevens worden opgeslagen; de nonce wordt opgeslagen in het manifest

### Geheimenbeheer

Enigma ondersteunt meerdere sleutelprovider-backends. Stel `key_provider` in de configuratie in:

| Provider | `key_provider` | Vereiste configuratie | Feature flag |
|----------|---------------|----------------------|-------------|
| Lokaal (standaard) | `"local"` | `keyfile_path` + passphrase | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

Cloud-inloggegevens in de configuratie kunnen worden versleuteld met `enigma encrypt-cred <value>` — produceert een `enc:...`-token om in TOML te plakken.

Aanvullende beveiliging:
- Bestandsrechten op `enigma.toml`
- Omgevingsvariabelen (`ENIGMA_PASSPHRASE`, AWS-omgevingsvariabelen, enz.)
- Het sleutelbestand zelf is versleuteld met de passphrase

## Snelstart

### Compileren

```bash
# Vereisten: Rust 1.85+, protoc (voor tonic/prost)
cargo build --release --workspace

# Met optionele features
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

# Locatie van binaries
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # S3-gateway
```

### CLI-gebruik

```bash
# Initialiseren (maakt configuratie + versleuteld sleutelbestand aan)
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# Back-up van een directory
enigma --passphrase "my-secret" backup /path/to/data

# Back-ups weergeven
enigma list

# Integriteit verifiëren
enigma --passphrase "my-secret" verify <backup-id>

# Herstellen (volledig)
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# Selectief herstel
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # prefixfilter
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # glob-filter
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # alleen bestanden weergeven

# Garbage collection
enigma gc --dry-run    # verweesde fragmenten weergeven
enigma gc              # verweesde fragmenten verwijderen

# Inloggegevens versleutelen voor configuratie
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

# Status / configuratie weergeven
enigma status
enigma config
```

### S3-gateway (enkelvoudige node)

```bash
# Proxy starten
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# Elke S3-client gebruiken
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## Configuratie

### Volledige referentie (`enigma.toml`)

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # voor azure-keyvault
# gcp_project_id = "my-project"                     # voor gcp-secretmanager
# aws_region = "us-east-1"                          # voor aws-secretsmanager
# secret_prefix = "enigma-key"                      # prefix voor vault-geheimennamen

# Fragmentatie — kies er een:
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 MB (standaard)

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 MB

# Compressie (optioneel, standaard uitgeschakeld)
[enigma.compression]
enabled = false                          # op true zetten om zstd in te schakelen
level = 3                                # zstd-niveau 1-22 (standaard: 3)

# S3-proxy (alleen enigma-proxy)
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"
# tls_cert = "/path/to/cert.pem"         # schakelt HTTPS in (feature: tls)
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # Prometheus-endpoint (feature: metrics)

# Opslagproviders — voeg er zoveel toe als nodig
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # Accepteert ook: "minio", "rustfs", "garage"
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
bucket = "enigma-container"              # Containernaam
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
bucket = "/data/enigma-local"            # Lokaal directorypad
weight = 1

# Raft (optioneel, voor multi-node HA)
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

### Providertypen

| Type | Waarde(n) | Opmerkingen |
|------|-----------|-------------|
| Lokaal bestandssysteem | `Local` | `bucket` = directorypad |
| AWS S3 | `S3` | Gebruikt de standaard AWS SDK-inlogketen |
| S3-compatibel | `S3Compatible`, `minio`, `rustfs`, `garage` | Vereist `endpoint_url`, `path_style = true` |
| Azure Blob Storage | `Azure` | `bucket` = containernaam |
| Google Cloud Storage | `Gcs` | Gebruikt Application Default Credentials |

### Omgevingsvariabelen

| Variabele | Beschrijving |
|-----------|-------------|
| `ENIGMA_PASSPHRASE` | Passphrase voor sleutelversleuteling (vermijdt interactieve prompt) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | AWS-inloggegevens (voor S3-provider) |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | Azure-inloggegevens |
| `GOOGLE_APPLICATION_CREDENTIALS` | Pad naar GCP-serviceaccount-JSON |
| `AWS_REGION` | AWS-regio voor Secrets Manager-sleutelprovider |
| `RUST_LOG` | Logniveaufilter (bijv. `enigma=info,tower=warn`) |

## S3 API-compatibiliteit

| Operatie | Ondersteund |
|----------|------------|
| CreateBucket | Ja |
| DeleteBucket | Ja (moet leeg zijn) |
| HeadBucket | Ja |
| ListBuckets | Ja |
| PutObject | Ja |
| GetObject | Ja |
| HeadObject | Ja |
| DeleteObject | Ja |
| ListObjectsV2 | Ja (prefix, delimiter, max-keys, continuation-token) |
| CreateMultipartUpload | Ja |
| UploadPart | Ja |
| CompleteMultipartUpload | Ja |
| AbortMultipartUpload | Ja |

## Tests

### Unit- & integratietests (49+ tests)

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

### Vault-tests (vereisen echte inloggegevens)

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

### Testdekking

| Module | Wat wordt getest |
|--------|-----------------|
| `chunk::cdc` | Leeg bestand, klein bestand (enkel fragment), groot bestand (meerdere fragmenten), deterministische hashes |
| `chunk::fixed` | Leeg bestand, exact veelvoud, restbehandeling |
| `compression` | Roundtrip compressie/decompressie, lege gegevens |
| `config` | TOML-roundtrip-serialisatie, fout bij ontbrekend bestand |
| `config::credentials` | Roundtrip versleuteling/ontsleuteling, platte-tekst-passthrough |
| `crypto` | Roundtrip versleuteling/ontsleuteling (raw + fragment), afwijzing verkeerde sleutel, afwijzing verkeerde AAD, unieke nonces |
| `dedup` | Deterministische hashing, verschillende gegevens = verschillende hashes, duplicaatdetectie |
| `distributor` | Round-robin-cyclus, gewogen distributie, providerzoektocht |
| `manifest::schema` | Tabelcreatie, migratie-idempotentie |
| `manifest::queries` | Volledige back-upstroom, lijstvolgorde, fragmentreferentietelling, logs |
| `types` | Hex-roundtrip ChunkHash, opslagsleutelformaat, KeyMaterial-nulstelling, ProviderType-parsing |
| `keys::local` | Sleutelbestand aanmaken/openen, verkeerde passphrase, ML-KEM-groottes, hybride sleutelonafhankelijkheid, rotatie |
| `keys::vault` | Azure KV, GCP SM, AWS SM — aanmaken, ophalen, roteren, weergeven (integratie) |
| `storage::local` | Verbindingstest, upload/download-roundtrip, manifest-roundtrip |

### Prestaties (Apple M3 Pro, release build)

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### Pipeline-doorvoer

| Fase | 1 MB | 4 MB | 16 MB |
|------|------|------|-------|
| SHA-256-hashing | 340 MB/s | 318 MB/s | 339 MB/s |
| AES-256-GCM-versleuteling | 135 MB/s | 135 MB/s | 137 MB/s |
| AES-256-GCM-ontsleuteling | 137 MB/s | 135 MB/s | 137 MB/s |
| zstd-compressie (willekeurig) | 4224 MB/s | 2484 MB/s | 1830 MB/s |
| zstd-compressie (tekst) | 6762 MB/s | 6242 MB/s | — |

#### Fragmentatie

| Engine | 4 MB bestand | 16 MB bestand | 64 MB bestand |
|--------|-------------|--------------|--------------|
| CDC (doel 4 MB) | 271 MB/s | 227 MB/s | 266 MB/s |
| Fixed (4 MB) | 308 MB/s | 221 MB/s | 310 MB/s |

#### Volledige pipeline (Fragmenteren -> Hashen -> Comprimeren -> Versleutelen)

| Invoer | Fragmenten | Doorvoer |
|--------|-----------|----------|
| 4 MB | 1 | 70 MB/s |
| 16 MB | 2-3 | 66 MB/s |
| 64 MB | 10-16 | 69 MB/s |

#### Sleutelafleiding (Argon2id + ML-KEM-768 + HKDF)

| Operatie | Tijd |
|----------|------|
| Aanmaken (keygen + versleuteling) | 17 ms |
| Openen (ontsleuteling + afleiding) | 15 ms |

> Knelpunt is AES-256-GCM (~135 MB/s). SHA-256 en zstd zijn veel sneller.
> Netwerk-I/O naar cloudbackends is in productie doorgaans het echte knelpunt.

### E2E-test

```bash
# Vereist 3 draaiende RustFS-instanties (Kind-cluster of docker-compose)
./tests/e2e_rustfs.sh
```

Tests: init -> back-up 5 bestanden -> verifiëren -> herstellen -> diff origineel vs hersteld.

### CI-pipeline

GitHub Actions draait bij elke push/PR naar `main`:
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## Implementatie

### Docker Compose (3-node cluster)

```bash
docker compose up -d
# 3 enigma-proxy nodes (poorten 8333-8335) + 3 RustFS-backends (poorten 19001-19003)

# Test
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes (StatefulSet)

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3 enigma-pods (StatefulSet) + 3 RustFS-deployments
# S3-toegang via enigma-s3 ClusterIP-service op poort 8333
```

### Enkele binary

```bash
# CLI-modus (back-up/herstel)
enigma --config-dir /etc/enigma backup /data

# Gateway-modus (S3-proxy)
enigma-proxy --config /etc/enigma/config.toml
```

## Roadmap

- [x] Vault-integratie voor geheimen (AWS Secrets Manager, Azure Key Vault, GCP Secret Manager)
- [x] Prometheus-metrics-endpoint
- [x] TLS-ondersteuning voor S3-gateway
- [x] Versleutelde inloggegevens in configuratie
- [x] Garbage collection voor verweesde fragmenten
- [x] Selectief herstel (path/glob-filters)
- [ ] Incrementele back-ups (alleen gewijzigde bestanden)
- [ ] Bandbreedtebeperking
- [ ] Web UI-dashboard
- [ ] Snapshot-gebaseerd Raft-herstel
- [ ] Erasure coding (Reed-Solomon) als alternatief voor replicatie

## Licentie

Source-Available (zie [LICENSE](LICENSE))
