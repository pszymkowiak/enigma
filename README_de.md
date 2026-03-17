[English](README.md) | [Français](README_fr.md) | [Español](README_es.md) | **Deutsch** | [Italiano](README_it.md) | [Português](README_pt.md) | [Nederlands](README_nl.md) | [Polski](README_pl.md) | [Русский](README_ru.md) | [日本語](README_ja.md) | [中文](README_zh.md) | [العربية](README_ar.md) | [한국어](README_ko.md)

# Enigma

Multi-Cloud-verschlüsseltes Backup-Tool mit S3-kompatibler Gateway und Raft-basierter Hochverfügbarkeit.

Enigma verschlüsselt, fragmentiert, dedupliziert, komprimiert optional und verteilt Daten über mehrere Cloud-Storage-Backends. Es stellt eine S3-kompatible API bereit, sodass jeder S3-Client (aws-cli, mc, rclone, SDKs) transparent damit interagieren kann.

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## Architektur

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

### Datenpipeline

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

Der Hash wird immer auf dem **originalen Klartext** berechnet, sodass die Deduplizierung identisch funktioniert, unabhängig davon, ob die Komprimierung aktiviert ist oder nicht. Die Spalte `size_compressed` im Manifest (NULL = nicht komprimiert) teilt dem Lesepfad mit, ob eine Dekomprimierung erforderlich ist — vollständig abwärtskompatibel.

### Crates

| Crate | Rolle |
|-------|-------|
| **enigma-core** | Fragmentierung (FastCDC / Fixed), Krypto (AES-256-GCM), Dedup (SHA-256), Komprimierung (zstd), Verteiler, Manifest (SQLite), Konfiguration (TOML) |
| **enigma-storage** | Trait `StorageProvider` + Implementierungen: Local, S3, S3-kompatibel, Azure Blob, GCS |
| **enigma-keys** | Trait `KeyProvider` + lokales hybrides Post-Quanten-Verfahren (Argon2id + ML-KEM-768), Azure Key Vault, GCP Secret Manager, AWS Secrets Manager |
| **enigma-cli** | CLI-Binärdatei (`enigma`) — init, backup, restore, verify, list, status, config, gc, encrypt-cred |
| **enigma-s3** | S3-Frontend aufgebaut auf s3s v0.11 — PutObject, GetObject, HeadObject, DeleteObject, ListObjectsV2, buckets, multipart |
| **enigma-raft** | Raft-Konsens (openraft v0.9 + tonic gRPC) — State Machine, die ManifestDb für HA-Metadatenreplikation umschließt |
| **enigma-proxy** | Binärdatei, die S3-Gateway + Raft kombiniert — Einzel-Knoten- oder Cluster-Modus |

## Funktionen

- **Ende-zu-Ende-Verschlüsselung** — AES-256-GCM, Schlüssel verlassen nie den Client
- **Hybride Post-Quanten-Schlüsselableitung** — Argon2id + ML-KEM-768 (FIPS 203) kombiniert über HKDF-SHA256
- **Inhaltsbasierte Fragmentierung** — FastCDC mit konfigurierbarer Zielgröße (Standard 4 MB) oder Fragmente fester Größe
- **SHA-256-Deduplizierung** — identische Fragmente werden nur einmal über alle Backups gespeichert
- **Optionale zstd-Komprimierung** — vor der Verschlüsselung angewendet, standardmäßig deaktiviert, abwärtskompatibel
- **Multi-Cloud-Verteilung** — Round-Robin oder gewichtete Verteilung über Anbieter
- **S3-kompatible Gateway** — vollständiges CRUD, Multipart-Uploads, ListObjectsV2 mit prefix/delimiter
- **Raft HA** — 3-Knoten-Konsens für Metadatenreplikation (Daten gehen direkt an die Backends)
- **Einzel-Knoten-Modus** — funktioniert ohne Raft, lokaler Speicher-Fallback wenn keine Anbieter konfiguriert
- **Vault-Schlüsselanbieter** — Azure Key Vault, GCP Secret Manager, AWS Secrets Manager (hinter Feature Flags)
- **TLS S3-Gateway** — optionales HTTPS mit rustls (PEM cert/key)
- **Prometheus-Metriken** — `/metrics`-Endpoint auf konfigurierbarem Port (hinter Feature `metrics`)
- **Verschlüsselte Anmeldedaten** — AES-256-GCM-verschlüsselte Geheimnisse in TOML-Konfiguration (Präfix `enc:`)
- **Garbage Collection** — `enigma gc` zum Finden und Löschen verwaister Fragmente (mit `--dry-run`)
- **Selektive Wiederherstellung** — `--path`, `--glob`, `--list`-Filter bei der Wiederherstellung
- **Audit-Trail** — SQLite-Manifest mit Backup-Protokollen und Fragment-Referenzzählung
- **Schlüsselrotation** — neue hybride Schlüssel generieren, alte Schlüssel bleiben über ID zugänglich

## Sicherheitsmodell

### Schlüsselableitung

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id**: speicherresistent, resistent gegen GPU/ASIC-Angriffe
- **ML-KEM-768**: NIST FIPS 203 Post-Quanten-KEM — schützt gegen zukünftige Quantencomputer
- **HKDF**: kombiniert beide Quellen; Sicherheit bleibt gewährleistet, wenn **eine** der Quellen unkompromittiert ist
- **Keystore auf der Festplatte**: `[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **Zeroisation**: alles Schlüsselmaterial wird bei der Zerstörung nullgesetzt (Crate `zeroize`)

### Verschlüsselung

- **AES-256-GCM** pro Fragment mit zufälligem 12-Byte-Nonce
- **AAD** (Additional Authenticated Data): SHA-256-Hash des Fragments — bindet den Chiffretext an seine Inhaltsidentität
- Verschlüsselte Daten werden gespeichert; der Nonce wird im Manifest gespeichert

### Geheimnisverwaltung

Enigma unterstützt mehrere Schlüsselanbieter-Backends. Setzen Sie `key_provider` in der Konfiguration:

| Anbieter | `key_provider` | Erforderliche Konfiguration | Feature Flag |
|----------|---------------|----------------------------|-------------|
| Lokal (Standard) | `"local"` | `keyfile_path` + Passphrase | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

Cloud-Anmeldedaten in der Konfiguration können mit `enigma encrypt-cred <value>` verschlüsselt werden — erzeugt ein `enc:...`-Token zum Einfügen in TOML.

Zusätzliche Sicherheit:
- Dateiberechtigungen auf `enigma.toml`
- Umgebungsvariablen (`ENIGMA_PASSPHRASE`, AWS-Umgebungsvariablen usw.)
- Die Schlüsseldatei selbst ist mit der Passphrase verschlüsselt

## Schnellstart

### Kompilierung

```bash
# Voraussetzungen: Rust 1.85+, protoc (für tonic/prost)
cargo build --release --workspace

# Mit optionalen Features
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

# Binärdateien-Speicherorte
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # S3-Gateway
```

### CLI-Verwendung

```bash
# Initialisieren (erstellt Konfiguration + verschlüsselte Schlüsseldatei)
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# Verzeichnis sichern
enigma --passphrase "my-secret" backup /path/to/data

# Backups auflisten
enigma list

# Integrität überprüfen
enigma --passphrase "my-secret" verify <backup-id>

# Wiederherstellen (vollständig)
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# Selektive Wiederherstellung
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # Präfixfilter
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # Glob-Filter
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # nur Dateien auflisten

# Garbage Collection
enigma gc --dry-run    # verwaiste Fragmente auflisten
enigma gc              # verwaiste Fragmente löschen

# Anmeldedaten für Konfiguration verschlüsseln
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

# Status / Konfiguration anzeigen
enigma status
enigma config
```

### S3-Gateway (Einzelknoten)

```bash
# Proxy starten
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# Beliebigen S3-Client verwenden
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## Konfiguration

### Vollständige Referenz (`enigma.toml`)

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # für azure-keyvault
# gcp_project_id = "my-project"                     # für gcp-secretmanager
# aws_region = "us-east-1"                          # für aws-secretsmanager
# secret_prefix = "enigma-key"                      # Präfix für Vault-Geheimnisnamen

# Fragmentierung — eines wählen:
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 MB (Standard)

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 MB

# Komprimierung (optional, standardmäßig deaktiviert)
[enigma.compression]
enabled = false                          # auf true setzen um zstd zu aktivieren
level = 3                                # zstd-Level 1-22 (Standard: 3)

# S3-Proxy (nur enigma-proxy)
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"
# tls_cert = "/path/to/cert.pem"         # aktiviert HTTPS (Feature: tls)
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # Prometheus-Endpoint (Feature: metrics)

# Speicheranbieter — so viele wie nötig hinzufügen
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # Akzeptiert auch: "minio", "rustfs", "garage"
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
bucket = "enigma-container"              # Container-Name
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
bucket = "/data/enigma-local"            # Lokaler Verzeichnispfad
weight = 1

# Raft (optional, für Multi-Knoten-HA)
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

### Anbietertypen

| Typ | Wert(e) | Hinweise |
|-----|---------|----------|
| Lokales Dateisystem | `Local` | `bucket` = Verzeichnispfad |
| AWS S3 | `S3` | Verwendet die Standard-AWS-SDK-Anmeldekette |
| S3-kompatibel | `S3Compatible`, `minio`, `rustfs`, `garage` | Erfordert `endpoint_url`, `path_style = true` |
| Azure Blob Storage | `Azure` | `bucket` = Container-Name |
| Google Cloud Storage | `Gcs` | Verwendet Application Default Credentials |

### Umgebungsvariablen

| Variable | Beschreibung |
|----------|-------------|
| `ENIGMA_PASSPHRASE` | Passphrase für die Schlüsselverschlüsselung (vermeidet interaktive Eingabeaufforderung) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | AWS-Anmeldedaten (für den S3-Anbieter) |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | Azure-Anmeldedaten |
| `GOOGLE_APPLICATION_CREDENTIALS` | Pfad zur GCP-Dienstkonto-JSON-Datei |
| `AWS_REGION` | AWS-Region für den Secrets Manager-Schlüsselanbieter |
| `RUST_LOG` | Log-Level-Filter (z.B. `enigma=info,tower=warn`) |

## S3-API-Kompatibilität

| Operation | Unterstützt |
|-----------|------------|
| CreateBucket | Ja |
| DeleteBucket | Ja (muss leer sein) |
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

### Unit- & Integrationstests (49+ Tests)

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

### Vault-Tests (erfordern echte Anmeldedaten)

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

### Testabdeckung

| Modul | Was getestet wird |
|-------|------------------|
| `chunk::cdc` | Leere Datei, kleine Datei (einzelnes Fragment), große Datei (mehrere Fragmente), deterministische Hashes |
| `chunk::fixed` | Leere Datei, exaktes Vielfaches, Restbehandlung |
| `compression` | Roundtrip Komprimierung/Dekomprimierung, leere Daten |
| `config` | TOML-Roundtrip-Serialisierung, Fehler bei fehlender Datei |
| `config::credentials` | Roundtrip Verschlüsselung/Entschlüsselung, Klartext-Passthrough |
| `crypto` | Roundtrip Verschlüsselung/Entschlüsselung (roh + Fragment), Ablehnung falscher Schlüssel, Ablehnung falscher AAD, einzigartige Nonces |
| `dedup` | Deterministisches Hashing, verschiedene Daten = verschiedene Hashes, Duplikaterkennung |
| `distributor` | Round-Robin-Zyklus, gewichtete Verteilung, Anbietersuche |
| `manifest::schema` | Tabellenerstellung, Migrations-Idempotenz |
| `manifest::queries` | Vollständiger Backup-Ablauf, Listenreihenfolge, Fragment-Referenzzählung, Protokolle |
| `types` | Hex-Roundtrip ChunkHash, Speicherschlüsselformat, KeyMaterial-Zeroisation, ProviderType-Parsing |
| `keys::local` | Schlüsseldatei erstellen/öffnen, falsche Passphrase, ML-KEM-Größen, hybride Schlüsselunabhängigkeit, Rotation |
| `keys::vault` | Azure KV, GCP SM, AWS SM — erstellen, abrufen, rotieren, auflisten (Integration) |
| `storage::local` | Verbindungstest, Upload/Download-Roundtrip, Manifest-Roundtrip |

### Leistung (Apple M3 Pro, Release-Build)

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### Pipeline-Durchsatz

| Stufe | 1 MB | 4 MB | 16 MB |
|-------|------|------|-------|
| SHA-256-Hashing | 340 MB/s | 318 MB/s | 339 MB/s |
| AES-256-GCM-Verschlüsselung | 135 MB/s | 135 MB/s | 137 MB/s |
| AES-256-GCM-Entschlüsselung | 137 MB/s | 135 MB/s | 137 MB/s |
| zstd-Komprimierung (zufällig) | 4224 MB/s | 2484 MB/s | 1830 MB/s |
| zstd-Komprimierung (Text) | 6762 MB/s | 6242 MB/s | — |

#### Fragmentierung

| Engine | 4 MB Datei | 16 MB Datei | 64 MB Datei |
|--------|-----------|------------|------------|
| CDC (Ziel 4 MB) | 271 MB/s | 227 MB/s | 266 MB/s |
| Fixed (4 MB) | 308 MB/s | 221 MB/s | 310 MB/s |

#### Vollständige Pipeline (Fragmentieren -> Hashen -> Komprimieren -> Verschlüsseln)

| Eingabe | Fragmente | Durchsatz |
|---------|----------|-----------|
| 4 MB | 1 | 70 MB/s |
| 16 MB | 2-3 | 66 MB/s |
| 64 MB | 10-16 | 69 MB/s |

#### Schlüsselableitung (Argon2id + ML-KEM-768 + HKDF)

| Operation | Zeit |
|-----------|------|
| Erstellen (Keygen + Verschlüsselung) | 17 ms |
| Öffnen (Entschlüsselung + Ableitung) | 15 ms |

> Engpass ist AES-256-GCM (~135 MB/s). SHA-256 und zstd sind deutlich schneller.
> Netzwerk-I/O zu Cloud-Backends ist in der Produktion typischerweise der eigentliche Engpass.

### E2E-Test

```bash
# Erfordert 3 laufende RustFS-Instanzen (Kind-Cluster oder docker-compose)
./tests/e2e_rustfs.sh
```

Tests: init -> 5 Dateien sichern -> verifizieren -> wiederherstellen -> diff Original vs. wiederhergestellt.

### CI-Pipeline

GitHub Actions wird bei jedem Push/PR auf `main` ausgeführt:
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## Bereitstellung

### Docker Compose (3-Knoten-Cluster)

```bash
docker compose up -d
# 3 enigma-proxy-Knoten (Ports 8333-8335) + 3 RustFS-Backends (Ports 19001-19003)

# Test
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes (StatefulSet)

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3 enigma-Pods (StatefulSet) + 3 RustFS-Deployments
# S3-Zugriff über enigma-s3 ClusterIP-Service auf Port 8333
```

### Einzelne Binärdatei

```bash
# CLI-Modus (Backup/Wiederherstellung)
enigma --config-dir /etc/enigma backup /data

# Gateway-Modus (S3-Proxy)
enigma-proxy --config /etc/enigma/config.toml
```

## Roadmap

- [x] Vault-Integration für Geheimnisse (AWS Secrets Manager, Azure Key Vault, GCP Secret Manager)
- [x] Prometheus-Metriken-Endpoint
- [x] TLS-Unterstützung für S3-Gateway
- [x] Verschlüsselte Anmeldedaten in der Konfiguration
- [x] Garbage Collection für verwaiste Fragmente
- [x] Selektive Wiederherstellung (Path/Glob-Filter)
- [ ] Inkrementelle Backups (nur geänderte Dateien)
- [ ] Bandbreitendrosselung
- [ ] Web-UI-Dashboard
- [ ] Snapshot-basierte Raft-Wiederherstellung
- [ ] Erasure Coding (Reed-Solomon) als Alternative zur Replikation

## Lizenz

Source-Available (siehe [LICENSE](LICENSE))
