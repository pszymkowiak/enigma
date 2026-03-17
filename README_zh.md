[English](README.md) | [Français](README_fr.md) | [Español](README_es.md) | [Deutsch](README_de.md) | [Italiano](README_it.md) | [Português](README_pt.md) | [Nederlands](README_nl.md) | [Polski](README_pl.md) | [Русский](README_ru.md) | [日本語](README_ja.md) | **中文** | [العربية](README_ar.md) | [한국어](README_ko.md)

# Enigma

多云加密备份工具，具有S3兼容网关和基于Raft的高可用性。

Enigma对数据进行加密、分块、去重、可选压缩，并分发到多个云存储后端。它公开了S3兼容API，因此任何S3客户端（aws-cli、mc、rclone、SDK）都可以透明地与其交互。

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## 架构

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

### 数据管道

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

哈希始终在**原始明文**上计算，因此无论是否启用压缩，去重都以相同方式工作。清单中的`size_compressed`列（NULL = 未压缩）告诉读取路径是否需要解压缩——完全向后兼容。

### Crate

| Crate | 角色 |
|-------|------|
| **enigma-core** | 分块（FastCDC / Fixed）、加密（AES-256-GCM）、去重（SHA-256）、压缩（zstd）、分发器、清单（SQLite）、配置（TOML） |
| **enigma-storage** | `StorageProvider` trait + 实现：Local、S3、S3兼容、Azure Blob、GCS |
| **enigma-keys** | `KeyProvider` trait + 本地混合后量子（Argon2id + ML-KEM-768）、Azure Key Vault、GCP Secret Manager、AWS Secrets Manager |
| **enigma-cli** | CLI二进制文件（`enigma`）— init、backup、restore、verify、list、status、config、gc、encrypt-cred |
| **enigma-s3** | 基于s3s v0.11构建的S3前端 — PutObject、GetObject、HeadObject、DeleteObject、ListObjectsV2、buckets、multipart |
| **enigma-raft** | Raft共识（openraft v0.9 + tonic gRPC）— 包装ManifestDb的状态机，用于HA元数据复制 |
| **enigma-proxy** | 组合S3网关 + Raft的二进制文件 — 单节点或集群模式 |

## 功能

- **端到端加密** — AES-256-GCM，密钥永不离开客户端
- **混合后量子密钥派生** — Argon2id + ML-KEM-768（FIPS 203）通过HKDF-SHA256组合
- **内容定义分块** — FastCDC，可配置目标大小（默认4 MB）或固定大小分块
- **SHA-256去重** — 相同分块在所有备份中仅存储一次
- **可选zstd压缩** — 在加密前应用，默认禁用，向后兼容
- **多云分发** — 提供商之间的轮询或加权分发
- **S3兼容网关** — 完整CRUD、分片上传、带prefix/delimiter的ListObjectsV2
- **Raft HA** — 用于元数据复制的3节点共识（数据直接发送到后端）
- **单节点模式** — 无需Raft即可工作，未配置提供商时回退到本地存储
- **Vault密钥提供商** — Azure Key Vault、GCP Secret Manager、AWS Secrets Manager（在feature flag后面）
- **TLS S3网关** — 使用rustls的可选HTTPS（PEM cert/key）
- **Prometheus指标** — 可配置端口上的`/metrics`端点（在`metrics` feature后面）
- **加密凭据** — TOML配置中的AES-256-GCM加密密钥（`enc:`前缀）
- **垃圾回收** — `enigma gc`查找并删除孤立分块（带`--dry-run`）
- **选择性恢复** — 恢复时的`--path`、`--glob`、`--list`过滤器
- **审计跟踪** — 带备份日志和分块引用计数的SQLite清单
- **密钥轮换** — 生成新的混合密钥，旧密钥仍可通过ID访问

## 安全模型

### 密钥派生

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id**：内存密集型，抵抗GPU/ASIC攻击
- **ML-KEM-768**：NIST FIPS 203后量子KEM — 防御未来量子计算机
- **HKDF**：组合两个来源；只要**任一**来源未被破坏，安全性即得到保障
- **磁盘上的密钥存储**：`[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **清零**：所有密钥材料在销毁时清零（`zeroize` crate）

### 加密

- 每个分块使用随机12字节nonce的**AES-256-GCM**
- **AAD**（附加认证数据）：分块的SHA-256哈希 — 将密文绑定到其内容身份
- 加密数据被存储；nonce存储在清单中

### 密钥管理

Enigma支持多种密钥提供商后端。在配置中设置`key_provider`：

| 提供商 | `key_provider` | 所需配置 | Feature flag |
|--------|---------------|---------|-------------|
| 本地（默认） | `"local"` | `keyfile_path` + 密码 | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

配置中的云凭据可以使用`enigma encrypt-cred <value>`加密 — 生成一个`enc:...`令牌粘贴到TOML中。

额外安全措施：
- `enigma.toml`的文件权限
- 环境变量（`ENIGMA_PASSPHRASE`、AWS环境变量等）
- 密钥文件本身用密码加密

## 快速开始

### 构建

```bash
# 前置条件：Rust 1.85+、protoc（用于tonic/prost）
cargo build --release --workspace

# 带可选feature
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

# 二进制文件位置
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # S3网关
```

### CLI使用

```bash
# 初始化（创建配置 + 加密密钥文件）
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# 备份目录
enigma --passphrase "my-secret" backup /path/to/data

# 列出备份
enigma list

# 验证完整性
enigma --passphrase "my-secret" verify <backup-id>

# 恢复（完整）
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# 选择性恢复
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # 前缀过滤
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # glob过滤
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # 仅列出文件

# 垃圾回收
enigma gc --dry-run    # 列出孤立分块
enigma gc              # 删除孤立分块

# 为配置加密凭据
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

# 显示状态 / 配置
enigma status
enigma config
```

### S3网关（单节点）

```bash
# 启动代理
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# 使用任何S3客户端
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## 配置

### 完整参考（`enigma.toml`）

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # 用于azure-keyvault
# gcp_project_id = "my-project"                     # 用于gcp-secretmanager
# aws_region = "us-east-1"                          # 用于aws-secretsmanager
# secret_prefix = "enigma-key"                      # vault密钥名称前缀

# 分块 — 选择其一：
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 MB（默认）

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 MB

# 压缩（可选，默认禁用）
[enigma.compression]
enabled = false                          # 设为true以启用zstd
level = 3                                # zstd级别 1-22（默认：3）

# S3代理（仅enigma-proxy）
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"
# tls_cert = "/path/to/cert.pem"         # 启用HTTPS（feature：tls）
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # Prometheus端点（feature：metrics）

# 存储提供商 — 按需添加
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # 也接受："minio"、"rustfs"、"garage"
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
bucket = "enigma-container"              # 容器名称
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
bucket = "/data/enigma-local"            # 本地目录路径
weight = 1

# Raft（可选，用于多节点HA）
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

### 提供商类型

| 类型 | 值 | 说明 |
|------|-----|------|
| 本地文件系统 | `Local` | `bucket` = 目录路径 |
| AWS S3 | `S3` | 使用AWS SDK默认凭据链 |
| S3兼容 | `S3Compatible`、`minio`、`rustfs`、`garage` | 需要`endpoint_url`、`path_style = true` |
| Azure Blob Storage | `Azure` | `bucket` = 容器名称 |
| Google Cloud Storage | `Gcs` | 使用Application Default Credentials |

### 环境变量

| 变量 | 描述 |
|------|------|
| `ENIGMA_PASSPHRASE` | 密钥加密密码（避免交互式提示） |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | AWS凭据（用于S3提供商） |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | Azure凭据 |
| `GOOGLE_APPLICATION_CREDENTIALS` | GCP服务账户JSON路径 |
| `AWS_REGION` | Secrets Manager密钥提供商的AWS区域 |
| `RUST_LOG` | 日志级别过滤器（例如：`enigma=info,tower=warn`） |

## S3 API兼容性

| 操作 | 支持 |
|------|------|
| CreateBucket | 是 |
| DeleteBucket | 是（必须为空） |
| HeadBucket | 是 |
| ListBuckets | 是 |
| PutObject | 是 |
| GetObject | 是 |
| HeadObject | 是 |
| DeleteObject | 是 |
| ListObjectsV2 | 是（prefix、delimiter、max-keys、continuation-token） |
| CreateMultipartUpload | 是 |
| UploadPart | 是 |
| CompleteMultipartUpload | 是 |
| AbortMultipartUpload | 是 |

## 测试

### 单元测试和集成测试（49+测试）

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

### Vault测试（需要真实凭据）

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

### 测试覆盖

| 模块 | 测试内容 |
|------|---------|
| `chunk::cdc` | 空文件、小文件（单个分块）、大文件（多分块）、确定性哈希 |
| `chunk::fixed` | 空文件、精确倍数、余数处理 |
| `compression` | 压缩/解压往返、空数据 |
| `config` | TOML序列化往返、文件缺失错误 |
| `config::credentials` | 加密/解密往返、明文直通 |
| `crypto` | 加密/解密往返（原始 + 分块）、错误密钥拒绝、错误AAD拒绝、唯一nonce |
| `dedup` | 确定性哈希、不同数据 = 不同哈希、重复检测 |
| `distributor` | 轮询循环、加权分发、提供商查找 |
| `manifest::schema` | 表创建、迁移幂等性 |
| `manifest::queries` | 完整备份流程、列表排序、分块引用计数、日志 |
| `types` | ChunkHash十六进制往返、存储键格式、KeyMaterial清零、ProviderType解析 |
| `keys::local` | 创建/打开密钥文件、错误密码、ML-KEM大小、混合密钥独立性、轮换 |
| `keys::vault` | Azure KV、GCP SM、AWS SM — 创建、获取、轮换、列出（集成） |
| `storage::local` | 连接测试、上传/下载往返、清单往返 |

### 性能（Apple M3 Pro，release构建）

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### 管道吞吐量

| 阶段 | 1 MB | 4 MB | 16 MB |
|------|------|------|-------|
| SHA-256哈希 | 340 MB/s | 318 MB/s | 339 MB/s |
| AES-256-GCM加密 | 135 MB/s | 135 MB/s | 137 MB/s |
| AES-256-GCM解密 | 137 MB/s | 135 MB/s | 137 MB/s |
| zstd压缩（随机） | 4224 MB/s | 2484 MB/s | 1830 MB/s |
| zstd压缩（文本） | 6762 MB/s | 6242 MB/s | — |

#### 分块

| 引擎 | 4 MB文件 | 16 MB文件 | 64 MB文件 |
|------|---------|----------|----------|
| CDC（目标4 MB） | 271 MB/s | 227 MB/s | 266 MB/s |
| Fixed（4 MB） | 308 MB/s | 221 MB/s | 310 MB/s |

#### 完整管道（分块 -> 哈希 -> 压缩 -> 加密）

| 输入 | 分块数 | 吞吐量 |
|------|-------|--------|
| 4 MB | 1 | 70 MB/s |
| 16 MB | 2-3 | 66 MB/s |
| 64 MB | 10-16 | 69 MB/s |

#### 密钥派生（Argon2id + ML-KEM-768 + HKDF）

| 操作 | 时间 |
|------|------|
| 创建（keygen + 加密） | 17 ms |
| 打开（解密 + 派生） | 15 ms |

> 瓶颈是AES-256-GCM（~135 MB/s）。SHA-256和zstd要快得多。
> 到云后端的网络I/O通常是生产环境中的真正瓶颈。

### E2E测试

```bash
# 需要3个运行中的RustFS实例（Kind集群或docker-compose）
./tests/e2e_rustfs.sh
```

测试：init -> 备份5个文件 -> 验证 -> 恢复 -> diff原始文件与恢复文件。

### CI管道

GitHub Actions在每次推送/PR到`main`时运行：
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## 部署

### Docker Compose（3节点集群）

```bash
docker compose up -d
# 3个enigma-proxy节点（端口8333-8335）+ 3个RustFS后端（端口19001-19003）

# 测试
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes（StatefulSet）

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3个enigma pod（StatefulSet）+ 3个RustFS deployment
# 通过端口8333的enigma-s3 ClusterIP服务访问S3
```

### 单一二进制文件

```bash
# CLI模式（备份/恢复）
enigma --config-dir /etc/enigma backup /data

# 网关模式（S3代理）
enigma-proxy --config /etc/enigma/config.toml
```

## 路线图

- [x] Vault集成用于密钥管理（AWS Secrets Manager、Azure Key Vault、GCP Secret Manager）
- [x] Prometheus指标端点
- [x] S3网关TLS支持
- [x] 配置中的加密凭据
- [x] 孤立分块的垃圾回收
- [x] 选择性恢复（path/glob过滤器）
- [ ] 增量备份（仅更改的文件）
- [ ] 带宽限制
- [ ] Web UI仪表板
- [ ] 基于快照的Raft恢复
- [ ] 纠删码（Reed-Solomon）作为复制的替代方案

## 许可证

Source-Available（见 [LICENSE](LICENSE)）
