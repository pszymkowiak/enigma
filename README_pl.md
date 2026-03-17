[English](README.md) | [Français](README_fr.md) | [Español](README_es.md) | [Deutsch](README_de.md) | [Italiano](README_it.md) | [Português](README_pt.md) | [Nederlands](README_nl.md) | **Polski** | [Русский](README_ru.md) | [日本語](README_ja.md) | [中文](README_zh.md) | [العربية](README_ar.md) | [한국어](README_ko.md)

# Enigma

Wielochmurowe narzędzie do szyfrowanych kopii zapasowych z bramą kompatybilną z S3 i wysoką dostępnością opartą na Raft.

Enigma szyfruje, fragmentuje, deduplikuje, opcjonalnie kompresuje i dystrybuuje dane na wiele backendów chmurowych. Udostępnia API kompatybilne z S3, dzięki czemu każdy klient S3 (aws-cli, mc, rclone, SDK) może z nim komunikować się w sposób przezroczysty.

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## Architektura

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

### Potok danych

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

Hash jest zawsze obliczany na **oryginalnym tekście jawnym**, więc deduplikacja działa identycznie niezależnie od tego, czy kompresja jest włączona. Kolumna `size_compressed` w manifeście (NULL = nieskompresowany) informuje ścieżkę odczytu, czy dekompresja jest potrzebna — w pełni wstecznie kompatybilne.

### Crate

| Crate | Rola |
|-------|------|
| **enigma-core** | Fragmentacja (FastCDC / Fixed), krypto (AES-256-GCM), dedup (SHA-256), kompresja (zstd), dystrybutor, manifest (SQLite), konfiguracja (TOML) |
| **enigma-storage** | Trait `StorageProvider` + implementacje: Local, S3, S3-kompatybilny, Azure Blob, GCS |
| **enigma-keys** | Trait `KeyProvider` + lokalny hybrydowy post-kwantowy (Argon2id + ML-KEM-768), Azure Key Vault, GCP Secret Manager, AWS Secrets Manager |
| **enigma-cli** | Plik binarny CLI (`enigma`) — init, backup, restore, verify, list, status, config, gc, encrypt-cred |
| **enigma-s3** | Frontend S3 zbudowany na s3s v0.11 — PutObject, GetObject, HeadObject, DeleteObject, ListObjectsV2, buckets, multipart |
| **enigma-raft** | Konsensus Raft (openraft v0.9 + tonic gRPC) — maszyna stanów opakowująca ManifestDb do replikacji metadanych HA |
| **enigma-proxy** | Plik binarny łączący bramę S3 + Raft — tryb pojedynczego węzła lub klastra |

## Funkcje

- **Szyfrowanie end-to-end** — AES-256-GCM, klucze nigdy nie opuszczają klienta
- **Hybrydowe wyprowadzanie klucza post-kwantowe** — Argon2id + ML-KEM-768 (FIPS 203) połączone przez HKDF-SHA256
- **Fragmentacja definiowana zawartością** — FastCDC z konfigurowalnym rozmiarem docelowym (domyślnie 4 MB) lub fragmenty o stałym rozmiarze
- **Deduplikacja SHA-256** — identyczne fragmenty przechowywane tylko raz we wszystkich kopiach zapasowych
- **Opcjonalna kompresja zstd** — stosowana przed szyfrowaniem, domyślnie wyłączona, wstecznie kompatybilna
- **Dystrybucja wielochmurowa** — round-robin lub dystrybucja ważona między dostawcami
- **Brama kompatybilna z S3** — pełne CRUD, przesyłanie wieloczęściowe, ListObjectsV2 z prefix/delimiter
- **Raft HA** — konsensus 3 węzłów do replikacji metadanych (dane trafiają bezpośrednio do backendów)
- **Tryb pojedynczego węzła** — działa bez Raft, lokalne przechowywanie awaryjne jeśli brak skonfigurowanych dostawców
- **Dostawcy kluczy Vault** — Azure Key Vault, GCP Secret Manager, AWS Secrets Manager (za flagami feature)
- **Brama S3 TLS** — opcjonalny HTTPS z rustls (certyfikat/klucz PEM)
- **Metryki Prometheus** — endpoint `/metrics` na konfigurowalnym porcie (za flagą feature `metrics`)
- **Zaszyfrowane poświadczenia** — sekrety zaszyfrowane AES-256-GCM w konfiguracji TOML (prefiks `enc:`)
- **Czyszczenie śmieci** — `enigma gc` do znajdowania i usuwania osieroconych fragmentów (z `--dry-run`)
- **Selektywne przywracanie** — filtry `--path`, `--glob`, `--list` przy przywracaniu
- **Ścieżka audytu** — manifest SQLite z logami kopii zapasowych i zliczaniem referencji fragmentów
- **Rotacja kluczy** — generowanie nowych kluczy hybrydowych, stare klucze pozostają dostępne po ID

## Model bezpieczeństwa

### Wyprowadzanie klucza

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id**: odporny na pamięć, odporny na ataki GPU/ASIC
- **ML-KEM-768**: NIST FIPS 203 post-kwantowy KEM — chroni przed przyszłymi komputerami kwantowymi
- **HKDF**: łączy oba źródła; bezpieczeństwo jest zachowane, jeśli **jedno z** źródeł nie jest skompromitowane
- **Keystore na dysku**: `[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **Zerowanie**: cały materiał kluczowy jest zerowany przy zniszczeniu (crate `zeroize`)

### Szyfrowanie

- **AES-256-GCM** na fragment z losowym 12-bajtowym nonce
- **AAD** (Additional Authenticated Data): hash SHA-256 fragmentu — wiąże szyfrogram z tożsamością jego zawartości
- Zaszyfrowane dane są przechowywane; nonce jest przechowywany w manifeście

### Zarządzanie sekretami

Enigma obsługuje wiele backendów dostawców kluczy. Ustaw `key_provider` w konfiguracji:

| Dostawca | `key_provider` | Wymagana konfiguracja | Flaga feature |
|----------|---------------|----------------------|--------------|
| Lokalny (domyślny) | `"local"` | `keyfile_path` + passphrase | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

Poświadczenia chmurowe w konfiguracji można zaszyfrować za pomocą `enigma encrypt-cred <value>` — generuje token `enc:...` do wklejenia w TOML.

Dodatkowe zabezpieczenia:
- Uprawnienia pliku na `enigma.toml`
- Zmienne środowiskowe (`ENIGMA_PASSPHRASE`, zmienne AWS itp.)
- Sam plik kluczy jest zaszyfrowany hasłem

## Szybki start

### Kompilacja

```bash
# Wymagania: Rust 1.85+, protoc (dla tonic/prost)
cargo build --release --workspace

# Z opcjonalnymi feature
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

# Lokalizacja plików binarnych
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # Brama S3
```

### Użycie CLI

```bash
# Inicjalizacja (tworzy konfigurację + zaszyfrowany plik kluczy)
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# Kopia zapasowa katalogu
enigma --passphrase "my-secret" backup /path/to/data

# Lista kopii zapasowych
enigma list

# Weryfikacja integralności
enigma --passphrase "my-secret" verify <backup-id>

# Przywracanie (pełne)
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# Selektywne przywracanie
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # filtr prefiksu
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # filtr glob
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # tylko lista plików

# Czyszczenie śmieci
enigma gc --dry-run    # lista osieroconych fragmentów
enigma gc              # usunięcie osieroconych fragmentów

# Zaszyfrowanie poświadczenia dla konfiguracji
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

# Wyświetlenie statusu / konfiguracji
enigma status
enigma config
```

### Brama S3 (pojedynczy węzeł)

```bash
# Uruchomienie proxy
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# Użycie dowolnego klienta S3
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## Konfiguracja

### Pełna dokumentacja (`enigma.toml`)

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # dla azure-keyvault
# gcp_project_id = "my-project"                     # dla gcp-secretmanager
# aws_region = "us-east-1"                          # dla aws-secretsmanager
# secret_prefix = "enigma-key"                      # prefiks dla nazw sekretów vault

# Fragmentacja — wybierz jedną:
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 MB (domyślnie)

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 MB

# Kompresja (opcjonalna, domyślnie wyłączona)
[enigma.compression]
enabled = false                          # ustaw na true aby włączyć zstd
level = 3                                # poziom zstd 1-22 (domyślnie: 3)

# Proxy S3 (tylko enigma-proxy)
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"
# tls_cert = "/path/to/cert.pem"         # włącza HTTPS (feature: tls)
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # endpoint Prometheus (feature: metrics)

# Dostawcy pamięci — dodaj tyle ile potrzeba
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # Akceptuje również: "minio", "rustfs", "garage"
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
bucket = "enigma-container"              # Nazwa kontenera
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
bucket = "/data/enigma-local"            # Ścieżka katalogu lokalnego
weight = 1

# Raft (opcjonalny, dla wielowęzłowej HA)
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

### Typy dostawców

| Typ | Wartość(ci) | Uwagi |
|-----|------------|-------|
| Lokalny system plików | `Local` | `bucket` = ścieżka katalogu |
| AWS S3 | `S3` | Używa domyślnego łańcucha poświadczeń AWS SDK |
| S3-kompatybilny | `S3Compatible`, `minio`, `rustfs`, `garage` | Wymaga `endpoint_url`, `path_style = true` |
| Azure Blob Storage | `Azure` | `bucket` = nazwa kontenera |
| Google Cloud Storage | `Gcs` | Używa Application Default Credentials |

### Zmienne środowiskowe

| Zmienna | Opis |
|---------|------|
| `ENIGMA_PASSPHRASE` | Hasło do szyfrowania kluczy (unika interaktywnego monitu) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | Poświadczenia AWS (dla dostawcy S3) |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | Poświadczenia Azure |
| `GOOGLE_APPLICATION_CREDENTIALS` | Ścieżka do JSON konta usługi GCP |
| `AWS_REGION` | Region AWS dla dostawcy kluczy Secrets Manager |
| `RUST_LOG` | Filtr poziomu logowania (np. `enigma=info,tower=warn`) |

## Kompatybilność API S3

| Operacja | Obsługiwana |
|----------|------------|
| CreateBucket | Tak |
| DeleteBucket | Tak (musi być pusty) |
| HeadBucket | Tak |
| ListBuckets | Tak |
| PutObject | Tak |
| GetObject | Tak |
| HeadObject | Tak |
| DeleteObject | Tak |
| ListObjectsV2 | Tak (prefix, delimiter, max-keys, continuation-token) |
| CreateMultipartUpload | Tak |
| UploadPart | Tak |
| CompleteMultipartUpload | Tak |
| AbortMultipartUpload | Tak |

## Testy

### Testy jednostkowe i integracyjne (49+ testów)

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

### Testy Vault (wymagają prawdziwych poświadczeń)

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

### Pokrycie testami

| Moduł | Co jest testowane |
|-------|------------------|
| `chunk::cdc` | Pusty plik, mały plik (pojedynczy fragment), duży plik (wiele fragmentów), deterministyczne hasze |
| `chunk::fixed` | Pusty plik, dokładna wielokrotność, obsługa reszty |
| `compression` | Roundtrip kompresja/dekompresja, puste dane |
| `config` | Roundtrip serializacji TOML, błąd brakującego pliku |
| `config::credentials` | Roundtrip szyfrowanie/deszyfrowanie, passthrough tekstu jawnego |
| `crypto` | Roundtrip szyfrowanie/deszyfrowanie (surowe + fragment), odrzucenie złego klucza, odrzucenie złego AAD, unikalne nonce |
| `dedup` | Deterministyczne haszowanie, różne dane = różne hasze, wykrywanie duplikatów |
| `distributor` | Cykl round-robin, dystrybucja ważona, wyszukiwanie dostawcy |
| `manifest::schema` | Tworzenie tabel, idempotencja migracji |
| `manifest::queries` | Pełny przepływ kopii zapasowej, kolejność listy, zliczanie referencji fragmentów, logi |
| `types` | Roundtrip hex ChunkHash, format klucza pamięci, zerowanie KeyMaterial, parsowanie ProviderType |
| `keys::local` | Tworzenie/otwieranie pliku kluczy, złe hasło, rozmiary ML-KEM, niezależność kluczy hybrydowych, rotacja |
| `keys::vault` | Azure KV, GCP SM, AWS SM — tworzenie, pobieranie, rotacja, lista (integracja) |
| `storage::local` | Test połączenia, roundtrip upload/download, roundtrip manifestu |

### Wydajność (Apple M3 Pro, build release)

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### Przepustowość potoku

| Etap | 1 MB | 4 MB | 16 MB |
|------|------|------|-------|
| Haszowanie SHA-256 | 340 MB/s | 318 MB/s | 339 MB/s |
| Szyfrowanie AES-256-GCM | 135 MB/s | 135 MB/s | 137 MB/s |
| Deszyfrowanie AES-256-GCM | 137 MB/s | 135 MB/s | 137 MB/s |
| Kompresja zstd (losowe) | 4224 MB/s | 2484 MB/s | 1830 MB/s |
| Kompresja zstd (tekst) | 6762 MB/s | 6242 MB/s | — |

#### Fragmentacja

| Silnik | Plik 4 MB | Plik 16 MB | Plik 64 MB |
|--------|----------|-----------|-----------|
| CDC (cel 4 MB) | 271 MB/s | 227 MB/s | 266 MB/s |
| Fixed (4 MB) | 308 MB/s | 221 MB/s | 310 MB/s |

#### Pełny potok (Fragmentacja -> Haszowanie -> Kompresja -> Szyfrowanie)

| Wejście | Fragmenty | Przepustowość |
|---------|----------|--------------|
| 4 MB | 1 | 70 MB/s |
| 16 MB | 2-3 | 66 MB/s |
| 64 MB | 10-16 | 69 MB/s |

#### Wyprowadzanie klucza (Argon2id + ML-KEM-768 + HKDF)

| Operacja | Czas |
|----------|------|
| Tworzenie (keygen + szyfrowanie) | 17 ms |
| Otwieranie (deszyfrowanie + wyprowadzanie) | 15 ms |

> Wąskim gardłem jest AES-256-GCM (~135 MB/s). SHA-256 i zstd są znacznie szybsze.
> Operacje sieciowe I/O do backendów chmurowych są zazwyczaj prawdziwym wąskim gardłem w produkcji.

### Test E2E

```bash
# Wymaga 3 działających instancji RustFS (klaster Kind lub docker-compose)
./tests/e2e_rustfs.sh
```

Testy: init -> kopia zapasowa 5 plików -> weryfikacja -> przywracanie -> diff oryginał vs przywrócone.

### Pipeline CI

GitHub Actions uruchamia się przy każdym push/PR do `main`:
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## Wdrożenie

### Docker Compose (klaster 3 węzłów)

```bash
docker compose up -d
# 3 węzły enigma-proxy (porty 8333-8335) + 3 backendy RustFS (porty 19001-19003)

# Test
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes (StatefulSet)

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3 pody enigma (StatefulSet) + 3 wdrożenia RustFS
# Dostęp S3 przez usługę ClusterIP enigma-s3 na porcie 8333
```

### Pojedynczy plik binarny

```bash
# Tryb CLI (kopia zapasowa/przywracanie)
enigma --config-dir /etc/enigma backup /data

# Tryb bramy (proxy S3)
enigma-proxy --config /etc/enigma/config.toml
```

## Plan rozwoju

- [x] Integracja Vault dla sekretów (AWS Secrets Manager, Azure Key Vault, GCP Secret Manager)
- [x] Endpoint metryk Prometheus
- [x] Obsługa TLS dla bramy S3
- [x] Zaszyfrowane poświadczenia w konfiguracji
- [x] Czyszczenie śmieci dla osieroconych fragmentów
- [x] Selektywne przywracanie (filtry path/glob)
- [ ] Przyrostowe kopie zapasowe (tylko zmienione pliki)
- [ ] Ograniczanie przepustowości
- [ ] Panel Web UI
- [ ] Odzyskiwanie Raft oparte na snapshotach
- [ ] Kodowanie wymazywania (Reed-Solomon) jako alternatywa dla replikacji

## Licencja

Source-Available (zobacz [LICENSE](LICENSE))
