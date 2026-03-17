[English](README.md) | [Français](README_fr.md) | [Español](README_es.md) | [Deutsch](README_de.md) | **Italiano** | [Português](README_pt.md) | [Nederlands](README_nl.md) | [Polski](README_pl.md) | [Русский](README_ru.md) | [日本語](README_ja.md) | [中文](README_zh.md) | [العربية](README_ar.md) | [한국어](README_ko.md)

# Enigma

Strumento di backup crittografato multi-cloud con gateway compatibile S3 e alta disponibilità basata su Raft.

Enigma crittografa, frammenta, deduplica, comprime opzionalmente e distribuisce i dati su più backend di archiviazione cloud. Espone un'API compatibile con S3 in modo che qualsiasi client S3 (aws-cli, mc, rclone, SDK) possa interagire in modo trasparente.

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## Architettura

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

### Pipeline dei dati

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

L'hash viene sempre calcolato sul **testo in chiaro originale**, quindi la deduplicazione funziona in modo identico sia che la compressione sia abilitata o meno. La colonna `size_compressed` nel manifesto (NULL = non compresso) indica al percorso di lettura se la decompressione è necessaria — completamente retrocompatibile.

### Crate

| Crate | Ruolo |
|-------|-------|
| **enigma-core** | Frammentazione (FastCDC / Fixed), crittografia (AES-256-GCM), dedup (SHA-256), compressione (zstd), distributore, manifesto (SQLite), configurazione (TOML) |
| **enigma-storage** | Trait `StorageProvider` + implementazioni: Local, S3, S3-compatibile, Azure Blob, GCS |
| **enigma-keys** | Trait `KeyProvider` + locale ibrido post-quantistico (Argon2id + ML-KEM-768), Azure Key Vault, GCP Secret Manager, AWS Secrets Manager |
| **enigma-cli** | Binario CLI (`enigma`) — init, backup, restore, verify, list, status, config, gc, encrypt-cred |
| **enigma-s3** | Frontend S3 costruito su s3s v0.11 — PutObject, GetObject, HeadObject, DeleteObject, ListObjectsV2, buckets, multipart |
| **enigma-raft** | Consenso Raft (openraft v0.9 + tonic gRPC) — macchina a stati che avvolge ManifestDb per la replica HA dei metadati |
| **enigma-proxy** | Binario che combina gateway S3 + Raft — modalità nodo singolo o cluster |

## Funzionalità

- **Crittografia end-to-end** — AES-256-GCM, le chiavi non lasciano mai il client
- **Derivazione chiave ibrida post-quantistica** — Argon2id + ML-KEM-768 (FIPS 203) combinati tramite HKDF-SHA256
- **Frammentazione definita dal contenuto** — FastCDC con dimensione target configurabile (predefinito 4 MB) o frammenti a dimensione fissa
- **Deduplicazione SHA-256** — i frammenti identici vengono memorizzati una sola volta in tutti i backup
- **Compressione zstd opzionale** — applicata prima della crittografia, disabilitata per impostazione predefinita, retrocompatibile
- **Distribuzione multi-cloud** — round-robin o distribuzione ponderata tra i provider
- **Gateway compatibile S3** — CRUD completo, upload multipart, ListObjectsV2 con prefix/delimiter
- **Raft HA** — consenso a 3 nodi per la replica dei metadati (i dati vanno direttamente ai backend)
- **Modalità nodo singolo** — funziona senza Raft, fallback su archiviazione locale se nessun provider è configurato
- **Provider di chiavi Vault** — Azure Key Vault, GCP Secret Manager, AWS Secrets Manager (dietro feature flag)
- **Gateway S3 TLS** — HTTPS opzionale con rustls (cert/chiave PEM)
- **Metriche Prometheus** — endpoint `/metrics` su porta configurabile (dietro la feature `metrics`)
- **Credenziali crittografate** — segreti crittografati con AES-256-GCM nella configurazione TOML (prefisso `enc:`)
- **Garbage collection** — `enigma gc` per trovare ed eliminare i frammenti orfani (con `--dry-run`)
- **Ripristino selettivo** — filtri `--path`, `--glob`, `--list` al ripristino
- **Traccia di audit** — manifesto SQLite con log di backup e conteggio dei riferimenti dei frammenti
- **Rotazione delle chiavi** — generare nuove chiavi ibride, le vecchie chiavi restano accessibili per ID

## Modello di sicurezza

### Derivazione delle chiavi

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id**: resistente alla memoria, resistente agli attacchi GPU/ASIC
- **ML-KEM-768**: NIST FIPS 203 KEM post-quantistico — protegge contro i futuri computer quantistici
- **HKDF**: combina entrambe le fonti; la sicurezza è garantita se **una qualsiasi** delle fonti non è compromessa
- **Keystore su disco**: `[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **Azzeramento**: tutto il materiale delle chiavi viene azzerato alla distruzione (crate `zeroize`)

### Crittografia

- **AES-256-GCM** per frammento con nonce casuale di 12 byte
- **AAD** (Additional Authenticated Data): hash SHA-256 del frammento — lega il testo cifrato alla sua identità di contenuto
- I dati crittografati vengono memorizzati; il nonce viene memorizzato nel manifesto

### Gestione dei segreti

Enigma supporta più backend di provider di chiavi. Impostare `key_provider` nella configurazione:

| Provider | `key_provider` | Configurazione richiesta | Feature flag |
|----------|---------------|-------------------------|-------------|
| Locale (predefinito) | `"local"` | `keyfile_path` + passphrase | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

Le credenziali cloud nella configurazione possono essere crittografate con `enigma encrypt-cred <value>` — produce un token `enc:...` da incollare nel TOML.

Sicurezza aggiuntiva:
- Permessi dei file su `enigma.toml`
- Variabili d'ambiente (`ENIGMA_PASSPHRASE`, variabili AWS, ecc.)
- Il file delle chiavi stesso è crittografato con la passphrase

## Avvio rapido

### Compilazione

```bash
# Prerequisiti: Rust 1.85+, protoc (per tonic/prost)
cargo build --release --workspace

# Con feature opzionali
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

# Posizione dei binari
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # Gateway S3
```

### Utilizzo CLI

```bash
# Inizializzazione (crea configurazione + file di chiavi crittografato)
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# Backup di una directory
enigma --passphrase "my-secret" backup /path/to/data

# Elencare i backup
enigma list

# Verificare l'integrità
enigma --passphrase "my-secret" verify <backup-id>

# Ripristinare (completo)
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# Ripristino selettivo
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # filtro per prefisso
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # filtro glob
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # elencare solo i file

# Garbage collection
enigma gc --dry-run    # elencare i frammenti orfani
enigma gc              # eliminare i frammenti orfani

# Crittografare una credenziale per la configurazione
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

# Mostrare stato / configurazione
enigma status
enigma config
```

### Gateway S3 (nodo singolo)

```bash
# Avviare il proxy
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# Usare qualsiasi client S3
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## Configurazione

### Riferimento completo (`enigma.toml`)

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # per azure-keyvault
# gcp_project_id = "my-project"                     # per gcp-secretmanager
# aws_region = "us-east-1"                          # per aws-secretsmanager
# secret_prefix = "enigma-key"                      # prefisso per i nomi dei segreti vault

# Frammentazione — sceglierne uno:
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 MB (predefinito)

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 MB

# Compressione (opzionale, disabilitata per impostazione predefinita)
[enigma.compression]
enabled = false                          # impostare su true per abilitare zstd
level = 3                                # livello zstd 1-22 (predefinito: 3)

# Proxy S3 (solo enigma-proxy)
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"
# tls_cert = "/path/to/cert.pem"         # abilita HTTPS (feature: tls)
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # endpoint Prometheus (feature: metrics)

# Provider di archiviazione — aggiungerne quanti necessari
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # Accetta anche: "minio", "rustfs", "garage"
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
bucket = "enigma-container"              # Nome del contenitore
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
bucket = "/data/enigma-local"            # Percorso della directory locale
weight = 1

# Raft (opzionale, per HA multi-nodo)
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

### Tipi di provider

| Tipo | Valore/i | Note |
|------|----------|------|
| File system locale | `Local` | `bucket` = percorso della directory |
| AWS S3 | `S3` | Usa la catena di credenziali predefinita di AWS SDK |
| S3-compatibile | `S3Compatible`, `minio`, `rustfs`, `garage` | Richiede `endpoint_url`, `path_style = true` |
| Azure Blob Storage | `Azure` | `bucket` = nome del contenitore |
| Google Cloud Storage | `Gcs` | Usa Application Default Credentials |

### Variabili d'ambiente

| Variabile | Descrizione |
|-----------|-------------|
| `ENIGMA_PASSPHRASE` | Passphrase per la crittografia delle chiavi (evita il prompt interattivo) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | Credenziali AWS (per il provider S3) |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | Credenziali Azure |
| `GOOGLE_APPLICATION_CREDENTIALS` | Percorso al JSON dell'account di servizio GCP |
| `AWS_REGION` | Regione AWS per il provider di chiavi Secrets Manager |
| `RUST_LOG` | Filtro livello di log (es: `enigma=info,tower=warn`) |

## Compatibilità API S3

| Operazione | Supportata |
|------------|-----------|
| CreateBucket | Sì |
| DeleteBucket | Sì (deve essere vuoto) |
| HeadBucket | Sì |
| ListBuckets | Sì |
| PutObject | Sì |
| GetObject | Sì |
| HeadObject | Sì |
| DeleteObject | Sì |
| ListObjectsV2 | Sì (prefix, delimiter, max-keys, continuation-token) |
| CreateMultipartUpload | Sì |
| UploadPart | Sì |
| CompleteMultipartUpload | Sì |
| AbortMultipartUpload | Sì |

## Test

### Test unitari e di integrazione (49+ test)

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

### Test Vault (richiedono credenziali reali)

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

### Copertura dei test

| Modulo | Cosa viene testato |
|--------|-------------------|
| `chunk::cdc` | File vuoto, file piccolo (frammento singolo), file grande (multi-frammento), hash deterministi |
| `chunk::fixed` | File vuoto, multiplo esatto, gestione del resto |
| `compression` | Roundtrip compressione/decompressione, dati vuoti |
| `config` | Serializzazione TOML roundtrip, errore file mancante |
| `config::credentials` | Roundtrip crittografia/decrittografia, passthrough testo in chiaro |
| `crypto` | Roundtrip crittografia/decrittografia (raw + frammento), rifiuto chiave errata, rifiuto AAD errato, nonce univoci |
| `dedup` | Hashing deterministico, dati diversi = hash diversi, rilevamento duplicati |
| `distributor` | Ciclo round-robin, distribuzione ponderata, ricerca provider |
| `manifest::schema` | Creazione tabelle, idempotenza migrazioni |
| `manifest::queries` | Flusso di backup completo, ordinamento lista, conteggio riferimenti frammenti, log |
| `types` | Roundtrip hex ChunkHash, formato chiave di archiviazione, azzeramento KeyMaterial, parsing ProviderType |
| `keys::local` | Creare/aprire file di chiavi, passphrase errata, dimensioni ML-KEM, indipendenza chiavi ibride, rotazione |
| `keys::vault` | Azure KV, GCP SM, AWS SM — creare, ottenere, ruotare, elencare (integrazione) |
| `storage::local` | Test di connessione, roundtrip upload/download, roundtrip manifesto |

### Prestazioni (Apple M3 Pro, build release)

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### Throughput della pipeline

| Fase | 1 MB | 4 MB | 16 MB |
|------|------|------|-------|
| Hashing SHA-256 | 340 MB/s | 318 MB/s | 339 MB/s |
| Crittografia AES-256-GCM | 135 MB/s | 135 MB/s | 137 MB/s |
| Decrittografia AES-256-GCM | 137 MB/s | 135 MB/s | 137 MB/s |
| Compressione zstd (casuale) | 4224 MB/s | 2484 MB/s | 1830 MB/s |
| Compressione zstd (testo) | 6762 MB/s | 6242 MB/s | — |

#### Frammentazione

| Motore | File 4 MB | File 16 MB | File 64 MB |
|--------|----------|-----------|-----------|
| CDC (target 4 MB) | 271 MB/s | 227 MB/s | 266 MB/s |
| Fixed (4 MB) | 308 MB/s | 221 MB/s | 310 MB/s |

#### Pipeline completa (Frammentare -> Hash -> Comprimere -> Crittografare)

| Input | Frammenti | Throughput |
|-------|----------|-----------|
| 4 MB | 1 | 70 MB/s |
| 16 MB | 2-3 | 66 MB/s |
| 64 MB | 10-16 | 69 MB/s |

#### Derivazione chiavi (Argon2id + ML-KEM-768 + HKDF)

| Operazione | Tempo |
|------------|-------|
| Creazione (keygen + crittografia) | 17 ms |
| Apertura (decrittografia + derivazione) | 15 ms |

> Il collo di bottiglia è AES-256-GCM (~135 MB/s). SHA-256 e zstd sono molto più veloci.
> L'I/O di rete verso i backend cloud è tipicamente il vero collo di bottiglia in produzione.

### Test E2E

```bash
# Richiede 3 istanze RustFS in esecuzione (cluster Kind o docker-compose)
./tests/e2e_rustfs.sh
```

Test: init -> backup 5 file -> verifica -> ripristino -> diff originale vs ripristinato.

### Pipeline CI

GitHub Actions viene eseguito ad ogni push/PR su `main`:
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## Distribuzione

### Docker Compose (cluster a 3 nodi)

```bash
docker compose up -d
# 3 nodi enigma-proxy (porte 8333-8335) + 3 backend RustFS (porte 19001-19003)

# Test
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes (StatefulSet)

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3 pod enigma (StatefulSet) + 3 deployment RustFS
# Accesso S3 tramite servizio ClusterIP enigma-s3 sulla porta 8333
```

### Binario singolo

```bash
# Modalità CLI (backup/ripristino)
enigma --config-dir /etc/enigma backup /data

# Modalità gateway (proxy S3)
enigma-proxy --config /etc/enigma/config.toml
```

## Roadmap

- [x] Integrazione Vault per i segreti (AWS Secrets Manager, Azure Key Vault, GCP Secret Manager)
- [x] Endpoint metriche Prometheus
- [x] Supporto TLS per il gateway S3
- [x] Credenziali crittografate nella configurazione
- [x] Garbage collection per frammenti orfani
- [x] Ripristino selettivo (filtri path/glob)
- [ ] Backup incrementali (solo file modificati)
- [ ] Limitazione della larghezza di banda
- [ ] Dashboard Web UI
- [ ] Recupero Raft basato su snapshot
- [ ] Erasure coding (Reed-Solomon) come alternativa alla replica

## Licenza

Source-Available (vedi [LICENSE](LICENSE))
