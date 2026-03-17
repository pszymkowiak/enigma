[English](README.md) | [Français](README_fr.md) | [Español](README_es.md) | [Deutsch](README_de.md) | [Italiano](README_it.md) | [Português](README_pt.md) | [Nederlands](README_nl.md) | [Polski](README_pl.md) | **Русский** | [日本語](README_ja.md) | [中文](README_zh.md) | [العربية](README_ar.md) | [한국어](README_ko.md)

# Enigma

Мультиоблачный инструмент для зашифрованного резервного копирования с S3-совместимым шлюзом и высокой доступностью на основе Raft.

Enigma шифрует, фрагментирует, дедуплицирует, опционально сжимает и распределяет данные по нескольким облачным бэкендам хранения. Он предоставляет S3-совместимый API, так что любой S3-клиент (aws-cli, mc, rclone, SDK) может прозрачно с ним взаимодействовать.

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## Архитектура

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

### Конвейер данных

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

Хеш всегда вычисляется по **исходному открытому тексту**, поэтому дедупликация работает одинаково независимо от того, включено ли сжатие. Столбец `size_compressed` в манифесте (NULL = не сжато) указывает пути чтения, нужна ли декомпрессия — полностью обратно совместимо.

### Крейты

| Крейт | Роль |
|-------|------|
| **enigma-core** | Фрагментация (FastCDC / Fixed), криптография (AES-256-GCM), дедупликация (SHA-256), сжатие (zstd), распределитель, манифест (SQLite), конфигурация (TOML) |
| **enigma-storage** | Трейт `StorageProvider` + реализации: Local, S3, S3-совместимый, Azure Blob, GCS |
| **enigma-keys** | Трейт `KeyProvider` + локальный гибридный постквантовый (Argon2id + ML-KEM-768), Azure Key Vault, GCP Secret Manager, AWS Secrets Manager |
| **enigma-cli** | CLI-бинарник (`enigma`) — init, backup, restore, verify, list, status, config, gc, encrypt-cred |
| **enigma-s3** | S3-фронтенд на s3s v0.11 — PutObject, GetObject, HeadObject, DeleteObject, ListObjectsV2, buckets, multipart |
| **enigma-raft** | Консенсус Raft (openraft v0.9 + tonic gRPC) — машина состояний, оборачивающая ManifestDb для HA-репликации метаданных |
| **enigma-proxy** | Бинарник, объединяющий S3-шлюз + Raft — одноузловой или кластерный режим |

## Возможности

- **Сквозное шифрование** — AES-256-GCM, ключи никогда не покидают клиент
- **Гибридное постквантовое выведение ключей** — Argon2id + ML-KEM-768 (FIPS 203) объединённые через HKDF-SHA256
- **Фрагментация по содержимому** — FastCDC с настраиваемым целевым размером (по умолчанию 4 МБ) или фрагменты фиксированного размера
- **Дедупликация SHA-256** — идентичные фрагменты хранятся только один раз для всех резервных копий
- **Опциональное сжатие zstd** — применяется до шифрования, отключено по умолчанию, обратно совместимо
- **Мультиоблачное распределение** — round-robin или взвешенное распределение между провайдерами
- **S3-совместимый шлюз** — полный CRUD, multipart загрузки, ListObjectsV2 с prefix/delimiter
- **Raft HA** — консенсус 3 узлов для репликации метаданных (данные идут напрямую в бэкенды)
- **Одноузловой режим** — работает без Raft, локальное хранилище при отсутствии настроенных провайдеров
- **Провайдеры ключей Vault** — Azure Key Vault, GCP Secret Manager, AWS Secrets Manager (за feature-флагами)
- **TLS S3-шлюз** — опциональный HTTPS с rustls (PEM cert/key)
- **Метрики Prometheus** — эндпоинт `/metrics` на настраиваемом порту (за feature `metrics`)
- **Зашифрованные учётные данные** — секреты, зашифрованные AES-256-GCM в конфигурации TOML (префикс `enc:`)
- **Сборка мусора** — `enigma gc` для поиска и удаления осиротевших фрагментов (с `--dry-run`)
- **Выборочное восстановление** — фильтры `--path`, `--glob`, `--list` при восстановлении
- **Аудит** — манифест SQLite с журналами резервного копирования и подсчётом ссылок фрагментов
- **Ротация ключей** — генерация новых гибридных ключей, старые ключи остаются доступными по ID

## Модель безопасности

### Выведение ключей

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id**: устойчив к памяти, устойчив к атакам GPU/ASIC
- **ML-KEM-768**: NIST FIPS 203 постквантовый KEM — защищает от будущих квантовых компьютеров
- **HKDF**: объединяет оба источника; безопасность сохраняется, если **любой** из источников не скомпрометирован
- **Хранилище ключей на диске**: `[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **Обнуление**: весь ключевой материал обнуляется при уничтожении (крейт `zeroize`)

### Шифрование

- **AES-256-GCM** для каждого фрагмента со случайным 12-байтовым nonce
- **AAD** (Additional Authenticated Data): хеш SHA-256 фрагмента — связывает шифротекст с идентичностью его содержимого
- Зашифрованные данные хранятся; nonce хранится в манифесте

### Управление секретами

Enigma поддерживает несколько бэкендов провайдеров ключей. Установите `key_provider` в конфигурации:

| Провайдер | `key_provider` | Необходимая конфигурация | Feature-флаг |
|-----------|---------------|-------------------------|-------------|
| Локальный (по умолчанию) | `"local"` | `keyfile_path` + passphrase | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

Облачные учётные данные в конфигурации могут быть зашифрованы с помощью `enigma encrypt-cred <value>` — создаёт токен `enc:...` для вставки в TOML.

Дополнительная безопасность:
- Права доступа к файлу `enigma.toml`
- Переменные окружения (`ENIGMA_PASSPHRASE`, переменные AWS и т.д.)
- Сам файл ключей зашифрован паролем

## Быстрый старт

### Сборка

```bash
# Требования: Rust 1.85+, protoc (для tonic/prost)
cargo build --release --workspace

# С опциональными feature
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

# Расположение бинарников
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # S3-шлюз
```

### Использование CLI

```bash
# Инициализация (создаёт конфигурацию + зашифрованный файл ключей)
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# Резервное копирование каталога
enigma --passphrase "my-secret" backup /path/to/data

# Список резервных копий
enigma list

# Проверка целостности
enigma --passphrase "my-secret" verify <backup-id>

# Восстановление (полное)
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# Выборочное восстановление
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # фильтр по префиксу
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # фильтр glob
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # только список файлов

# Сборка мусора
enigma gc --dry-run    # список осиротевших фрагментов
enigma gc              # удаление осиротевших фрагментов

# Шифрование учётных данных для конфигурации
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

# Показать статус / конфигурацию
enigma status
enigma config
```

### S3-шлюз (одиночный узел)

```bash
# Запуск прокси
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# Использование любого S3-клиента
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## Конфигурация

### Полная документация (`enigma.toml`)

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # для azure-keyvault
# gcp_project_id = "my-project"                     # для gcp-secretmanager
# aws_region = "us-east-1"                          # для aws-secretsmanager
# secret_prefix = "enigma-key"                      # префикс для имён секретов vault

# Фрагментация — выберите один:
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 МБ (по умолчанию)

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 МБ

# Сжатие (опционально, по умолчанию отключено)
[enigma.compression]
enabled = false                          # установите в true для включения zstd
level = 3                                # уровень zstd 1-22 (по умолчанию: 3)

# S3-прокси (только enigma-proxy)
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"
# tls_cert = "/path/to/cert.pem"         # включает HTTPS (feature: tls)
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # эндпоинт Prometheus (feature: metrics)

# Провайдеры хранения — добавляйте столько, сколько нужно
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # Также принимает: "minio", "rustfs", "garage"
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
bucket = "enigma-container"              # Имя контейнера
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
bucket = "/data/enigma-local"            # Путь к локальному каталогу
weight = 1

# Raft (опционально, для многоузловой HA)
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

### Типы провайдеров

| Тип | Значение(я) | Примечания |
|-----|------------|-----------|
| Локальная файловая система | `Local` | `bucket` = путь к каталогу |
| AWS S3 | `S3` | Использует цепочку учётных данных AWS SDK по умолчанию |
| S3-совместимый | `S3Compatible`, `minio`, `rustfs`, `garage` | Требует `endpoint_url`, `path_style = true` |
| Azure Blob Storage | `Azure` | `bucket` = имя контейнера |
| Google Cloud Storage | `Gcs` | Использует Application Default Credentials |

### Переменные окружения

| Переменная | Описание |
|-----------|---------|
| `ENIGMA_PASSPHRASE` | Пароль для шифрования ключей (избегает интерактивного запроса) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | Учётные данные AWS (для провайдера S3) |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | Учётные данные Azure |
| `GOOGLE_APPLICATION_CREDENTIALS` | Путь к JSON сервисного аккаунта GCP |
| `AWS_REGION` | Регион AWS для провайдера ключей Secrets Manager |
| `RUST_LOG` | Фильтр уровня логирования (например, `enigma=info,tower=warn`) |

## Совместимость S3 API

| Операция | Поддерживается |
|----------|---------------|
| CreateBucket | Да |
| DeleteBucket | Да (должен быть пустым) |
| HeadBucket | Да |
| ListBuckets | Да |
| PutObject | Да |
| GetObject | Да |
| HeadObject | Да |
| DeleteObject | Да |
| ListObjectsV2 | Да (prefix, delimiter, max-keys, continuation-token) |
| CreateMultipartUpload | Да |
| UploadPart | Да |
| CompleteMultipartUpload | Да |
| AbortMultipartUpload | Да |

## Тесты

### Модульные и интеграционные тесты (49+ тестов)

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

### Тесты Vault (требуют реальных учётных данных)

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

### Покрытие тестами

| Модуль | Что тестируется |
|--------|----------------|
| `chunk::cdc` | Пустой файл, маленький файл (один фрагмент), большой файл (несколько фрагментов), детерминированные хеши |
| `chunk::fixed` | Пустой файл, точное кратное, обработка остатка |
| `compression` | Roundtrip сжатие/декомпрессия, пустые данные |
| `config` | Roundtrip сериализации TOML, ошибка отсутствующего файла |
| `config::credentials` | Roundtrip шифрование/дешифрование, passthrough открытого текста |
| `crypto` | Roundtrip шифрование/дешифрование (raw + фрагмент), отклонение неверного ключа, отклонение неверного AAD, уникальные nonce |
| `dedup` | Детерминированное хеширование, разные данные = разные хеши, обнаружение дубликатов |
| `distributor` | Цикл round-robin, взвешенное распределение, поиск провайдера |
| `manifest::schema` | Создание таблиц, идемпотентность миграций |
| `manifest::queries` | Полный поток резервного копирования, порядок списка, подсчёт ссылок фрагментов, журналы |
| `types` | Roundtrip hex ChunkHash, формат ключа хранения, обнуление KeyMaterial, парсинг ProviderType |
| `keys::local` | Создание/открытие файла ключей, неверный пароль, размеры ML-KEM, независимость гибридных ключей, ротация |
| `keys::vault` | Azure KV, GCP SM, AWS SM — создание, получение, ротация, список (интеграция) |
| `storage::local` | Тест подключения, roundtrip upload/download, roundtrip манифеста |

### Производительность (Apple M3 Pro, release build)

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### Пропускная способность конвейера

| Этап | 1 МБ | 4 МБ | 16 МБ |
|------|------|------|-------|
| Хеширование SHA-256 | 340 МБ/с | 318 МБ/с | 339 МБ/с |
| Шифрование AES-256-GCM | 135 МБ/с | 135 МБ/с | 137 МБ/с |
| Дешифрование AES-256-GCM | 137 МБ/с | 135 МБ/с | 137 МБ/с |
| Сжатие zstd (случайные) | 4224 МБ/с | 2484 МБ/с | 1830 МБ/с |
| Сжатие zstd (текст) | 6762 МБ/с | 6242 МБ/с | — |

#### Фрагментация

| Движок | Файл 4 МБ | Файл 16 МБ | Файл 64 МБ |
|--------|----------|-----------|-----------|
| CDC (цель 4 МБ) | 271 МБ/с | 227 МБ/с | 266 МБ/с |
| Fixed (4 МБ) | 308 МБ/с | 221 МБ/с | 310 МБ/с |

#### Полный конвейер (Фрагментация -> Хеширование -> Сжатие -> Шифрование)

| Вход | Фрагменты | Пропускная способность |
|------|----------|----------------------|
| 4 МБ | 1 | 70 МБ/с |
| 16 МБ | 2-3 | 66 МБ/с |
| 64 МБ | 10-16 | 69 МБ/с |

#### Выведение ключей (Argon2id + ML-KEM-768 + HKDF)

| Операция | Время |
|----------|-------|
| Создание (keygen + шифрование) | 17 мс |
| Открытие (дешифрование + выведение) | 15 мс |

> Узкое место — AES-256-GCM (~135 МБ/с). SHA-256 и zstd значительно быстрее.
> Сетевые операции ввода-вывода к облачным бэкендам обычно являются реальным узким местом в продакшене.

### Тест E2E

```bash
# Требуются 3 работающих экземпляра RustFS (кластер Kind или docker-compose)
./tests/e2e_rustfs.sh
```

Тесты: init -> резервное копирование 5 файлов -> проверка -> восстановление -> diff оригинал vs восстановленное.

### CI-конвейер

GitHub Actions запускается при каждом push/PR в `main`:
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## Развёртывание

### Docker Compose (кластер из 3 узлов)

```bash
docker compose up -d
# 3 узла enigma-proxy (порты 8333-8335) + 3 бэкенда RustFS (порты 19001-19003)

# Тест
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes (StatefulSet)

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3 пода enigma (StatefulSet) + 3 деплоймента RustFS
# Доступ по S3 через сервис ClusterIP enigma-s3 на порту 8333
```

### Один бинарник

```bash
# Режим CLI (резервное копирование/восстановление)
enigma --config-dir /etc/enigma backup /data

# Режим шлюза (S3-прокси)
enigma-proxy --config /etc/enigma/config.toml
```

## Дорожная карта

- [x] Интеграция с Vault для секретов (AWS Secrets Manager, Azure Key Vault, GCP Secret Manager)
- [x] Эндпоинт метрик Prometheus
- [x] Поддержка TLS для S3-шлюза
- [x] Зашифрованные учётные данные в конфигурации
- [x] Сборка мусора для осиротевших фрагментов
- [x] Выборочное восстановление (фильтры path/glob)
- [ ] Инкрементальные резервные копии (только изменённые файлы)
- [ ] Ограничение пропускной способности
- [ ] Веб-панель управления
- [ ] Восстановление Raft на основе снимков
- [ ] Помехоустойчивое кодирование (Reed-Solomon) как альтернатива репликации

## Лицензия

Source-Available (см. [LICENSE](LICENSE))
