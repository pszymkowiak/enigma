[English](README.md) | [Français](README_fr.md) | [Español](README_es.md) | [Deutsch](README_de.md) | [Italiano](README_it.md) | **Português** | [Nederlands](README_nl.md) | [Polski](README_pl.md) | [Русский](README_ru.md) | [日本語](README_ja.md) | [中文](README_zh.md) | [العربية](README_ar.md) | [한국어](README_ko.md)

# Enigma

Ferramenta de backup criptografado multi-cloud com gateway compatível com S3 e alta disponibilidade baseada em Raft.

Enigma criptografa, fragmenta, deduplica, comprime opcionalmente e distribui dados por múltiplos backends de armazenamento em nuvem. Expõe uma API compatível com S3 para que qualquer cliente S3 (aws-cli, mc, rclone, SDKs) possa interagir de forma transparente.

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## Arquitetura

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

### Pipeline de dados

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

O hash é sempre calculado no **texto claro original**, então a deduplicação funciona de forma idêntica independentemente de a compressão estar ativada ou não. A coluna `size_compressed` no manifesto (NULL = não comprimido) indica ao caminho de leitura se a descompressão é necessária — totalmente retrocompatível.

### Crates

| Crate | Função |
|-------|--------|
| **enigma-core** | Fragmentação (FastCDC / Fixed), criptografia (AES-256-GCM), dedup (SHA-256), compressão (zstd), distribuidor, manifesto (SQLite), configuração (TOML) |
| **enigma-storage** | Trait `StorageProvider` + implementações: Local, S3, S3-compatível, Azure Blob, GCS |
| **enigma-keys** | Trait `KeyProvider` + local híbrido pós-quântico (Argon2id + ML-KEM-768), Azure Key Vault, GCP Secret Manager, AWS Secrets Manager |
| **enigma-cli** | Binário CLI (`enigma`) — init, backup, restore, verify, list, status, config, gc, encrypt-cred |
| **enigma-s3** | Frontend S3 construído sobre s3s v0.11 — PutObject, GetObject, HeadObject, DeleteObject, ListObjectsV2, buckets, multipart |
| **enigma-raft** | Consenso Raft (openraft v0.9 + tonic gRPC) — máquina de estados envolvendo ManifestDb para replicação HA de metadados |
| **enigma-proxy** | Binário combinando gateway S3 + Raft — modo nó único ou cluster |

## Funcionalidades

- **Criptografia ponta a ponta** — AES-256-GCM, as chaves nunca saem do cliente
- **Derivação de chave híbrida pós-quântica** — Argon2id + ML-KEM-768 (FIPS 203) combinados via HKDF-SHA256
- **Fragmentação definida por conteúdo** — FastCDC com tamanho alvo configurável (padrão 4 MB) ou fragmentos de tamanho fixo
- **Deduplicação SHA-256** — fragmentos idênticos armazenados apenas uma vez em todos os backups
- **Compressão zstd opcional** — aplicada antes da criptografia, desativada por padrão, retrocompatível
- **Distribuição multi-cloud** — round-robin ou distribuição ponderada entre provedores
- **Gateway compatível com S3** — CRUD completo, uploads multipart, ListObjectsV2 com prefix/delimiter
- **Raft HA** — consenso de 3 nós para replicação de metadados (os dados vão diretamente para os backends)
- **Modo nó único** — funciona sem Raft, fallback para armazenamento local se nenhum provedor estiver configurado
- **Provedores de chaves Vault** — Azure Key Vault, GCP Secret Manager, AWS Secrets Manager (atrás de feature flags)
- **Gateway S3 TLS** — HTTPS opcional com rustls (cert/chave PEM)
- **Métricas Prometheus** — endpoint `/metrics` em porta configurável (atrás da feature `metrics`)
- **Credenciais criptografadas** — segredos criptografados com AES-256-GCM na configuração TOML (prefixo `enc:`)
- **Coleta de lixo** — `enigma gc` para encontrar e excluir fragmentos órfãos (com `--dry-run`)
- **Restauração seletiva** — filtros `--path`, `--glob`, `--list` na restauração
- **Trilha de auditoria** — manifesto SQLite com logs de backup e contagem de referências dos fragmentos
- **Rotação de chaves** — gerar novas chaves híbridas, chaves antigas permanecem acessíveis por ID

## Modelo de segurança

### Derivação de chave

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id**: resistente à memória, resistente a ataques GPU/ASIC
- **ML-KEM-768**: NIST FIPS 203 KEM pós-quântico — protege contra futuros computadores quânticos
- **HKDF**: combina ambas as fontes; a segurança é mantida se **qualquer uma** das fontes não estiver comprometida
- **Keystore no disco**: `[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **Zeragem**: todo o material de chaves é zerado na destruição (crate `zeroize`)

### Criptografia

- **AES-256-GCM** por fragmento com nonce aleatório de 12 bytes
- **AAD** (Additional Authenticated Data): hash SHA-256 do fragmento — vincula o texto cifrado à sua identidade de conteúdo
- Os dados criptografados são armazenados; o nonce é armazenado no manifesto

### Gestão de segredos

Enigma suporta múltiplos backends de provedores de chaves. Configure `key_provider` na configuração:

| Provedor | `key_provider` | Configuração necessária | Feature flag |
|----------|---------------|------------------------|-------------|
| Local (padrão) | `"local"` | `keyfile_path` + passphrase | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

As credenciais cloud na configuração podem ser criptografadas com `enigma encrypt-cred <value>` — produz um token `enc:...` para colar no TOML.

Segurança adicional:
- Permissões de arquivo em `enigma.toml`
- Variáveis de ambiente (`ENIGMA_PASSPHRASE`, variáveis AWS, etc.)
- O próprio arquivo de chaves é criptografado com a passphrase

## Início rápido

### Compilação

```bash
# Pré-requisitos: Rust 1.85+, protoc (para tonic/prost)
cargo build --release --workspace

# Com features opcionais
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

# Localização dos binários
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # Gateway S3
```

### Uso do CLI

```bash
# Inicializar (cria configuração + arquivo de chaves criptografado)
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# Fazer backup de um diretório
enigma --passphrase "my-secret" backup /path/to/data

# Listar backups
enigma list

# Verificar integridade
enigma --passphrase "my-secret" verify <backup-id>

# Restaurar (completo)
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# Restauração seletiva
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # filtro por prefixo
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # filtro glob
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # listar apenas arquivos

# Coleta de lixo
enigma gc --dry-run    # listar fragmentos órfãos
enigma gc              # excluir fragmentos órfãos

# Criptografar uma credencial para a configuração
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

# Mostrar status / configuração
enigma status
enigma config
```

### Gateway S3 (nó único)

```bash
# Iniciar o proxy
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# Usar qualquer cliente S3
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## Configuração

### Referência completa (`enigma.toml`)

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # para azure-keyvault
# gcp_project_id = "my-project"                     # para gcp-secretmanager
# aws_region = "us-east-1"                          # para aws-secretsmanager
# secret_prefix = "enigma-key"                      # prefixo para nomes de segredos vault

# Fragmentação — escolher um:
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 MB (padrão)

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 MB

# Compressão (opcional, desativada por padrão)
[enigma.compression]
enabled = false                          # definir como true para ativar zstd
level = 3                                # nível zstd 1-22 (padrão: 3)

# Proxy S3 (apenas enigma-proxy)
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"
# tls_cert = "/path/to/cert.pem"         # ativa HTTPS (feature: tls)
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # endpoint Prometheus (feature: metrics)

# Provedores de armazenamento — adicione quantos forem necessários
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # Também aceita: "minio", "rustfs", "garage"
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
bucket = "enigma-container"              # Nome do contêiner
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
bucket = "/data/enigma-local"            # Caminho do diretório local
weight = 1

# Raft (opcional, para HA multi-nó)
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

### Tipos de provedores

| Tipo | Valor(es) | Notas |
|------|-----------|-------|
| Sistema de arquivos local | `Local` | `bucket` = caminho do diretório |
| AWS S3 | `S3` | Usa a cadeia de credenciais padrão do AWS SDK |
| S3-compatível | `S3Compatible`, `minio`, `rustfs`, `garage` | Requer `endpoint_url`, `path_style = true` |
| Azure Blob Storage | `Azure` | `bucket` = nome do contêiner |
| Google Cloud Storage | `Gcs` | Usa Application Default Credentials |

### Variáveis de ambiente

| Variável | Descrição |
|----------|-----------|
| `ENIGMA_PASSPHRASE` | Passphrase para criptografia de chaves (evita o prompt interativo) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | Credenciais AWS (para o provedor S3) |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | Credenciais Azure |
| `GOOGLE_APPLICATION_CREDENTIALS` | Caminho para o JSON da conta de serviço GCP |
| `AWS_REGION` | Região AWS para o provedor de chaves Secrets Manager |
| `RUST_LOG` | Filtro de nível de log (ex: `enigma=info,tower=warn`) |

## Compatibilidade API S3

| Operação | Suportada |
|----------|-----------|
| CreateBucket | Sim |
| DeleteBucket | Sim (deve estar vazio) |
| HeadBucket | Sim |
| ListBuckets | Sim |
| PutObject | Sim |
| GetObject | Sim |
| HeadObject | Sim |
| DeleteObject | Sim |
| ListObjectsV2 | Sim (prefix, delimiter, max-keys, continuation-token) |
| CreateMultipartUpload | Sim |
| UploadPart | Sim |
| CompleteMultipartUpload | Sim |
| AbortMultipartUpload | Sim |

## Testes

### Testes unitários e de integração (49+ testes)

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

### Testes Vault (requerem credenciais reais)

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

### Cobertura dos testes

| Módulo | O que é testado |
|--------|----------------|
| `chunk::cdc` | Arquivo vazio, arquivo pequeno (fragmento único), arquivo grande (multi-fragmento), hashes determinísticos |
| `chunk::fixed` | Arquivo vazio, múltiplo exato, tratamento de resto |
| `compression` | Roundtrip compressão/descompressão, dados vazios |
| `config` | Serialização TOML roundtrip, erro de arquivo ausente |
| `config::credentials` | Roundtrip criptografia/descriptografia, passthrough texto claro |
| `crypto` | Roundtrip criptografia/descriptografia (raw + fragmento), rejeição de chave errada, rejeição de AAD errado, nonces únicos |
| `dedup` | Hashing determinístico, dados diferentes = hashes diferentes, detecção de duplicados |
| `distributor` | Ciclo round-robin, distribuição ponderada, busca de provedor |
| `manifest::schema` | Criação de tabelas, idempotência de migrações |
| `manifest::queries` | Fluxo completo de backup, ordenação de lista, contagem de referências de fragmentos, logs |
| `types` | Roundtrip hex ChunkHash, formato de chave de armazenamento, zeragem de KeyMaterial, parsing de ProviderType |
| `keys::local` | Criar/abrir arquivo de chaves, passphrase errada, tamanhos ML-KEM, independência de chaves híbridas, rotação |
| `keys::vault` | Azure KV, GCP SM, AWS SM — criar, obter, rotacionar, listar (integração) |
| `storage::local` | Teste de conexão, roundtrip upload/download, roundtrip manifesto |

### Desempenho (Apple M3 Pro, build release)

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### Throughput do pipeline

| Etapa | 1 MB | 4 MB | 16 MB |
|-------|------|------|-------|
| Hashing SHA-256 | 340 MB/s | 318 MB/s | 339 MB/s |
| Criptografia AES-256-GCM | 135 MB/s | 135 MB/s | 137 MB/s |
| Descriptografia AES-256-GCM | 137 MB/s | 135 MB/s | 137 MB/s |
| Compressão zstd (aleatório) | 4224 MB/s | 2484 MB/s | 1830 MB/s |
| Compressão zstd (texto) | 6762 MB/s | 6242 MB/s | — |

#### Fragmentação

| Motor | Arquivo 4 MB | Arquivo 16 MB | Arquivo 64 MB |
|-------|-------------|--------------|--------------|
| CDC (alvo 4 MB) | 271 MB/s | 227 MB/s | 266 MB/s |
| Fixed (4 MB) | 308 MB/s | 221 MB/s | 310 MB/s |

#### Pipeline completo (Fragmentar -> Hash -> Comprimir -> Criptografar)

| Entrada | Fragmentos | Throughput |
|---------|-----------|-----------|
| 4 MB | 1 | 70 MB/s |
| 16 MB | 2-3 | 66 MB/s |
| 64 MB | 10-16 | 69 MB/s |

#### Derivação de chave (Argon2id + ML-KEM-768 + HKDF)

| Operação | Tempo |
|----------|-------|
| Criação (keygen + criptografia) | 17 ms |
| Abertura (descriptografia + derivação) | 15 ms |

> O gargalo é AES-256-GCM (~135 MB/s). SHA-256 e zstd são muito mais rápidos.
> A E/S de rede para backends em nuvem é tipicamente o verdadeiro gargalo em produção.

### Teste E2E

```bash
# Requer 3 instâncias RustFS em execução (cluster Kind ou docker-compose)
./tests/e2e_rustfs.sh
```

Testes: init -> backup 5 arquivos -> verificação -> restauração -> diff original vs restaurado.

### Pipeline CI

GitHub Actions executa a cada push/PR para `main`:
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## Implantação

### Docker Compose (cluster de 3 nós)

```bash
docker compose up -d
# 3 nós enigma-proxy (portas 8333-8335) + 3 backends RustFS (portas 19001-19003)

# Teste
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes (StatefulSet)

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3 pods enigma (StatefulSet) + 3 deployments RustFS
# Acesso S3 via serviço ClusterIP enigma-s3 na porta 8333
```

### Binário único

```bash
# Modo CLI (backup/restauração)
enigma --config-dir /etc/enigma backup /data

# Modo gateway (proxy S3)
enigma-proxy --config /etc/enigma/config.toml
```

## Roadmap

- [x] Integração Vault para segredos (AWS Secrets Manager, Azure Key Vault, GCP Secret Manager)
- [x] Endpoint de métricas Prometheus
- [x] Suporte TLS para o gateway S3
- [x] Credenciais criptografadas na configuração
- [x] Coleta de lixo para fragmentos órfãos
- [x] Restauração seletiva (filtros path/glob)
- [ ] Backups incrementais (apenas arquivos alterados)
- [ ] Limitação de largura de banda
- [ ] Painel Web UI
- [ ] Recuperação Raft baseada em snapshots
- [ ] Erasure coding (Reed-Solomon) como alternativa à replicação

## Licença

Source-Available (veja [LICENSE](LICENSE))
