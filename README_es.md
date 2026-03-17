[English](README.md) | [Français](README_fr.md) | **Español** | [Deutsch](README_de.md) | [Italiano](README_it.md) | [Português](README_pt.md) | [Nederlands](README_nl.md) | [Polski](README_pl.md) | [Русский](README_ru.md) | [日本語](README_ja.md) | [中文](README_zh.md) | [العربية](README_ar.md) | [한국어](README_ko.md)

# Enigma

Herramienta de copia de seguridad cifrada multi-nube con pasarela compatible con S3 y alta disponibilidad basada en Raft.

Enigma cifra, fragmenta, deduplica, comprime opcionalmente y distribuye datos a través de múltiples backends de almacenamiento en la nube. Expone una API compatible con S3 para que cualquier cliente S3 (aws-cli, mc, rclone, SDKs) pueda interactuar con él de forma transparente.

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## Arquitectura

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

### Pipeline de datos

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

El hash siempre se calcula sobre el **texto plano original**, por lo que la deduplicación funciona de manera idéntica tanto si la compresión está activada como si no. La columna `size_compressed` en el manifiesto (NULL = no comprimido) indica a la ruta de lectura si se necesita descompresión — totalmente retrocompatible.

### Crates

| Crate | Rol |
|-------|-----|
| **enigma-core** | Fragmentación (FastCDC / Fixed), crypto (AES-256-GCM), dedup (SHA-256), compresión (zstd), distribuidor, manifiesto (SQLite), config (TOML) |
| **enigma-storage** | Trait `StorageProvider` + implementaciones: Local, S3, S3-compatible, Azure Blob, GCS |
| **enigma-keys** | Trait `KeyProvider` + local híbrido post-cuántico (Argon2id + ML-KEM-768), Azure Key Vault, GCP Secret Manager, AWS Secrets Manager |
| **enigma-cli** | Binario CLI (`enigma`) — init, backup, restore, verify, list, status, config, gc, encrypt-cred |
| **enigma-s3** | Frontend S3 construido sobre s3s v0.11 — PutObject, GetObject, HeadObject, DeleteObject, ListObjectsV2, buckets, multipart |
| **enigma-raft** | Consenso Raft (openraft v0.9 + tonic gRPC) — máquina de estados envolviendo ManifestDb para replicación HA de metadatos |
| **enigma-proxy** | Binario que combina pasarela S3 + Raft — modo nodo único o clúster |

## Características

- **Cifrado de extremo a extremo** — AES-256-GCM, las claves nunca salen del cliente
- **Derivación de clave híbrida post-cuántica** — Argon2id + ML-KEM-768 (FIPS 203) combinados via HKDF-SHA256
- **Fragmentación definida por contenido** — FastCDC con tamaño objetivo configurable (por defecto 4 MB) o fragmentos de tamaño fijo
- **Deduplicación SHA-256** — los fragmentos idénticos se almacenan solo una vez en todas las copias de seguridad
- **Compresión zstd opcional** — aplicada antes del cifrado, desactivada por defecto, retrocompatible
- **Distribución multi-nube** — round-robin o distribución ponderada entre proveedores
- **Pasarela compatible con S3** — CRUD completo, uploads multipart, ListObjectsV2 con prefix/delimiter
- **Raft HA** — consenso de 3 nodos para replicación de metadatos (los datos van directamente a los backends)
- **Modo nodo único** — funciona sin Raft, respaldo en almacenamiento local si no hay proveedores configurados
- **Proveedores de claves Vault** — Azure Key Vault, GCP Secret Manager, AWS Secrets Manager (detrás de feature flags)
- **Pasarela S3 TLS** — HTTPS opcional con rustls (cert/clave PEM)
- **Métricas Prometheus** — endpoint `/metrics` en puerto configurable (detrás de la feature `metrics`)
- **Credenciales cifradas** — secretos cifrados con AES-256-GCM en la config TOML (prefijo `enc:`)
- **Recolección de basura** — `enigma gc` para encontrar y eliminar fragmentos huérfanos (con `--dry-run`)
- **Restauración selectiva** — filtros `--path`, `--glob`, `--list` en la restauración
- **Rastro de auditoría** — manifiesto SQLite con registros de copia de seguridad y conteo de referencias de fragmentos
- **Rotación de claves** — generar nuevas claves híbridas, las claves antiguas permanecen accesibles por ID

## Modelo de seguridad

### Derivación de clave

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id**: resistente a la memoria, resistente a ataques GPU/ASIC
- **ML-KEM-768**: NIST FIPS 203 post-cuántico KEM — protege contra futuros ordenadores cuánticos
- **HKDF**: combina ambas fuentes; la seguridad se mantiene si **cualquiera** de las fuentes no está comprometida
- **Keystore en disco**: `[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **Zeroización**: todo el material de claves se zeroiza al destruirse (crate `zeroize`)

### Cifrado

- **AES-256-GCM** por fragmento con nonce aleatorio de 12 bytes
- **AAD** (Additional Authenticated Data): hash SHA-256 del fragmento — vincula el texto cifrado a la identidad de su contenido
- Los datos cifrados se almacenan; el nonce se almacena en el manifiesto

### Gestión de secretos

Enigma soporta múltiples backends de proveedores de claves. Configure `key_provider` en la config:

| Proveedor | `key_provider` | Config requerida | Feature flag |
|-----------|---------------|-----------------|-------------|
| Local (defecto) | `"local"` | `keyfile_path` + passphrase | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

Las credenciales en la nube en la config pueden cifrarse con `enigma encrypt-cred <value>` — produce un token `enc:...` para pegar en el TOML.

Seguridad adicional:
- Permisos de archivo en `enigma.toml`
- Variables de entorno (`ENIGMA_PASSPHRASE`, variables de entorno AWS, etc.)
- El propio archivo de claves está cifrado con la passphrase

## Inicio rápido

### Compilación

```bash
# Requisitos previos: Rust 1.85+, protoc (para tonic/prost)
cargo build --release --workspace

# Con features opcionales
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

# Ubicación de los binarios
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # Pasarela S3
```

### Uso del CLI

```bash
# Inicializar (crea config + archivo de claves cifrado)
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# Hacer copia de seguridad de un directorio
enigma --passphrase "my-secret" backup /path/to/data

# Listar copias de seguridad
enigma list

# Verificar integridad
enigma --passphrase "my-secret" verify <backup-id>

# Restaurar (completo)
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# Restauración selectiva
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # filtro por prefijo
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # filtro glob
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # listar solo archivos

# Recolección de basura
enigma gc --dry-run    # listar fragmentos huérfanos
enigma gc              # eliminar fragmentos huérfanos

# Cifrar una credencial para la config
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

# Mostrar estado / config
enigma status
enigma config
```

### Pasarela S3 (nodo único)

```bash
# Iniciar el proxy
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# Usar cualquier cliente S3
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## Configuración

### Referencia completa (`enigma.toml`)

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # para azure-keyvault
# gcp_project_id = "my-project"                     # para gcp-secretmanager
# aws_region = "us-east-1"                          # para aws-secretsmanager
# secret_prefix = "enigma-key"                      # prefijo para nombres de secretos vault

# Fragmentación — elegir uno:
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 MB (defecto)

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 MB

# Compresión (opcional, desactivada por defecto)
[enigma.compression]
enabled = false                          # poner a true para activar zstd
level = 3                                # nivel zstd 1-22 (defecto: 3)

# Proxy S3 (solo enigma-proxy)
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"
# tls_cert = "/path/to/cert.pem"         # activa HTTPS (feature: tls)
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # endpoint Prometheus (feature: metrics)

# Proveedores de almacenamiento — agregue tantos como necesite
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # También acepta: "minio", "rustfs", "garage"
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
bucket = "enigma-container"              # Nombre del contenedor
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
bucket = "/data/enigma-local"            # Ruta del directorio local
weight = 1

# Raft (opcional, para HA multi-nodo)
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

### Tipos de proveedores

| Tipo | Valor(es) | Notas |
|------|-----------|-------|
| Sistema de archivos local | `Local` | `bucket` = ruta del directorio |
| AWS S3 | `S3` | Usa la cadena de credenciales por defecto de AWS SDK |
| S3-compatible | `S3Compatible`, `minio`, `rustfs`, `garage` | Requiere `endpoint_url`, `path_style = true` |
| Azure Blob Storage | `Azure` | `bucket` = nombre del contenedor |
| Google Cloud Storage | `Gcs` | Usa Application Default Credentials |

### Variables de entorno

| Variable | Descripción |
|----------|-------------|
| `ENIGMA_PASSPHRASE` | Passphrase para el cifrado de claves (evita el prompt interactivo) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | Credenciales AWS (para el proveedor S3) |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | Credenciales Azure |
| `GOOGLE_APPLICATION_CREDENTIALS` | Ruta al JSON de la cuenta de servicio GCP |
| `AWS_REGION` | Región AWS para el proveedor de claves Secrets Manager |
| `RUST_LOG` | Filtro de nivel de log (ej: `enigma=info,tower=warn`) |

## Compatibilidad API S3

| Operación | Soportada |
|-----------|-----------|
| CreateBucket | Sí |
| DeleteBucket | Sí (debe estar vacío) |
| HeadBucket | Sí |
| ListBuckets | Sí |
| PutObject | Sí |
| GetObject | Sí |
| HeadObject | Sí |
| DeleteObject | Sí |
| ListObjectsV2 | Sí (prefix, delimiter, max-keys, continuation-token) |
| CreateMultipartUpload | Sí |
| UploadPart | Sí |
| CompleteMultipartUpload | Sí |
| AbortMultipartUpload | Sí |

## Tests

### Tests unitarios y de integración (49+ tests)

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

### Tests de Vault (requieren credenciales reales)

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

### Cobertura de tests

| Módulo | Qué se prueba |
|--------|--------------|
| `chunk::cdc` | Archivo vacío, archivo pequeño (fragmento único), archivo grande (multi-fragmento), hashes deterministas |
| `chunk::fixed` | Archivo vacío, múltiplo exacto, manejo de residuo |
| `compression` | Ida y vuelta compress/decompress, datos vacíos |
| `config` | Serialización TOML ida y vuelta, error archivo faltante |
| `config::credentials` | Ida y vuelta cifrado/descifrado, passthrough texto plano |
| `crypto` | Ida y vuelta cifrado/descifrado (raw + fragmento), rechazo clave incorrecta, rechazo AAD incorrecto, nonces únicos |
| `dedup` | Hashing determinista, datos diferentes = hashes diferentes, detección de duplicados |
| `distributor` | Ciclo round-robin, distribución ponderada, búsqueda de proveedor |
| `manifest::schema` | Creación de tablas, idempotencia de migraciones |
| `manifest::queries` | Flujo completo de copia de seguridad, orden de listado, conteo de referencias de fragmentos, registros |
| `types` | Ida y vuelta hex ChunkHash, formato clave de almacenamiento, zeroización KeyMaterial, parsing ProviderType |
| `keys::local` | Crear/abrir archivo de claves, passphrase incorrecta, tamaños ML-KEM, independencia de claves híbridas, rotación |
| `keys::vault` | Azure KV, GCP SM, AWS SM — crear, obtener, rotar, listar (integración) |
| `storage::local` | Test de conexión, ida y vuelta upload/download, ida y vuelta manifiesto |

### Rendimiento (Apple M3 Pro, build release)

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### Rendimiento del pipeline

| Etapa | 1 MB | 4 MB | 16 MB |
|-------|------|------|-------|
| Hashing SHA-256 | 340 MB/s | 318 MB/s | 339 MB/s |
| Cifrado AES-256-GCM | 135 MB/s | 135 MB/s | 137 MB/s |
| Descifrado AES-256-GCM | 137 MB/s | 135 MB/s | 137 MB/s |
| Compresión zstd (aleatorio) | 4224 MB/s | 2484 MB/s | 1830 MB/s |
| Compresión zstd (texto) | 6762 MB/s | 6242 MB/s | — |

#### Fragmentación

| Motor | Archivo 4 MB | Archivo 16 MB | Archivo 64 MB |
|-------|-------------|--------------|--------------|
| CDC (objetivo 4 MB) | 271 MB/s | 227 MB/s | 266 MB/s |
| Fixed (4 MB) | 308 MB/s | 221 MB/s | 310 MB/s |

#### Pipeline completo (Fragmentar -> Hashear -> Comprimir -> Cifrar)

| Entrada | Fragmentos | Rendimiento |
|---------|-----------|------------|
| 4 MB | 1 | 70 MB/s |
| 16 MB | 2-3 | 66 MB/s |
| 64 MB | 10-16 | 69 MB/s |

#### Derivación de clave (Argon2id + ML-KEM-768 + HKDF)

| Operación | Tiempo |
|-----------|--------|
| Creación (keygen + cifrado) | 17 ms |
| Apertura (descifrado + derivación) | 15 ms |

> El cuello de botella es AES-256-GCM (~135 MB/s). SHA-256 y zstd son mucho más rápidos.
> Las E/S de red hacia los backends en la nube suelen ser el verdadero cuello de botella en producción.

### Test E2E

```bash
# Requiere 3 instancias RustFS en ejecución (clúster Kind o docker-compose)
./tests/e2e_rustfs.sh
```

Tests: init -> copia de seguridad 5 archivos -> verificación -> restauración -> diff original vs restaurado.

### Pipeline CI

GitHub Actions se ejecuta en cada push/PR a `main`:
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## Despliegue

### Docker Compose (clúster de 3 nodos)

```bash
docker compose up -d
# 3 nodos enigma-proxy (puertos 8333-8335) + 3 backends RustFS (puertos 19001-19003)

# Test
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes (StatefulSet)

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3 pods enigma (StatefulSet) + 3 despliegues RustFS
# Acceso S3 via servicio ClusterIP enigma-s3 en el puerto 8333
```

### Binario único

```bash
# Modo CLI (copia de seguridad/restauración)
enigma --config-dir /etc/enigma backup /data

# Modo pasarela (proxy S3)
enigma-proxy --config /etc/enigma/config.toml
```

## Hoja de ruta

- [x] Integración Vault para secretos (AWS Secrets Manager, Azure Key Vault, GCP Secret Manager)
- [x] Endpoint de métricas Prometheus
- [x] Soporte TLS para la pasarela S3
- [x] Credenciales cifradas en la config
- [x] Recolección de basura para fragmentos huérfanos
- [x] Restauración selectiva (filtros path/glob)
- [ ] Copias de seguridad incrementales (solo archivos modificados)
- [ ] Limitación de ancho de banda
- [ ] Panel de control Web UI
- [ ] Recuperación Raft basada en snapshots
- [ ] Codificación de borrado (Reed-Solomon) como alternativa a la replicación

## Licencia

Source-Available (ver [LICENSE](LICENSE))
