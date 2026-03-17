[English](README.md) | [Français](README_fr.md) | [Español](README_es.md) | [Deutsch](README_de.md) | [Italiano](README_it.md) | [Português](README_pt.md) | [Nederlands](README_nl.md) | [Polski](README_pl.md) | [Русский](README_ru.md) | [日本語](README_ja.md) | [中文](README_zh.md) | [العربية](README_ar.md) | **한국어**

# Enigma

S3 호환 게이트웨이와 Raft 기반 고가용성을 갖춘 멀티클라우드 암호화 백업 도구.

Enigma는 데이터를 암호화, 청킹, 중복 제거, 선택적 압축하여 여러 클라우드 스토리지 백엔드에 분산합니다. S3 호환 API를 제공하여 모든 S3 클라이언트(aws-cli, mc, rclone, SDK)가 투명하게 상호작용할 수 있습니다.

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## 아키텍처

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

### 데이터 파이프라인

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

해시는 항상 **원본 평문**에서 계산되므로, 압축 활성화 여부와 관계없이 중복 제거가 동일하게 작동합니다. 매니페스트의 `size_compressed` 컬럼(NULL = 비압축)이 읽기 경로에 압축 해제 필요 여부를 알려줍니다 — 완벽하게 하위 호환됩니다.

### 크레이트

| 크레이트 | 역할 |
|---------|------|
| **enigma-core** | 청킹(FastCDC / Fixed), 암호화(AES-256-GCM), 중복 제거(SHA-256), 압축(zstd), 분배기, 매니페스트(SQLite), 설정(TOML) |
| **enigma-storage** | `StorageProvider` 트레이트 + 구현: Local, S3, S3 호환, Azure Blob, GCS |
| **enigma-keys** | `KeyProvider` 트레이트 + 로컬 하이브리드 포스트양자(Argon2id + ML-KEM-768), Azure Key Vault, GCP Secret Manager, AWS Secrets Manager |
| **enigma-cli** | CLI 바이너리(`enigma`) — init, backup, restore, verify, list, status, config, gc, encrypt-cred |
| **enigma-s3** | s3s v0.11 기반 S3 프론트엔드 — PutObject, GetObject, HeadObject, DeleteObject, ListObjectsV2, buckets, multipart |
| **enigma-raft** | Raft 합의(openraft v0.9 + tonic gRPC) — HA 메타데이터 복제를 위해 ManifestDb를 래핑하는 상태 머신 |
| **enigma-proxy** | S3 게이트웨이 + Raft를 결합한 바이너리 — 단일 노드 또는 클러스터 모드 |

## 기능

- **종단간 암호화** — AES-256-GCM, 키가 클라이언트를 절대 벗어나지 않음
- **하이브리드 포스트양자 키 파생** — Argon2id + ML-KEM-768(FIPS 203)을 HKDF-SHA256으로 결합
- **콘텐츠 정의 청킹** — 설정 가능한 목표 크기(기본 4 MB)의 FastCDC 또는 고정 크기 청크
- **SHA-256 중복 제거** — 동일한 청크는 모든 백업에서 한 번만 저장
- **선택적 zstd 압축** — 암호화 전에 적용, 기본적으로 비활성화, 하위 호환
- **멀티클라우드 분산** — 프로바이더 간 라운드로빈 또는 가중 분산
- **S3 호환 게이트웨이** — 전체 CRUD, 멀티파트 업로드, prefix/delimiter 지원 ListObjectsV2
- **Raft HA** — 메타데이터 복제를 위한 3노드 합의(데이터는 백엔드로 직접 전송)
- **단일 노드 모드** — Raft 없이 작동, 프로바이더 미설정 시 로컬 스토리지로 폴백
- **Vault 키 프로바이더** — Azure Key Vault, GCP Secret Manager, AWS Secrets Manager(feature flag 뒤)
- **TLS S3 게이트웨이** — rustls를 사용한 선택적 HTTPS(PEM cert/key)
- **Prometheus 메트릭** — 설정 가능한 포트의 `/metrics` 엔드포인트(`metrics` feature 뒤)
- **암호화된 자격 증명** — TOML 설정의 AES-256-GCM 암호화 시크릿(`enc:` 접두사)
- **가비지 컬렉션** — `enigma gc`로 고아 청크 검색 및 삭제(`--dry-run` 지원)
- **선택적 복원** — 복원 시 `--path`, `--glob`, `--list` 필터
- **감사 추적** — 백업 로그와 청크 참조 카운팅이 포함된 SQLite 매니페스트
- **키 순환** — 새 하이브리드 키 생성, 이전 키는 ID로 계속 접근 가능

## 보안 모델

### 키 파생

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id**: 메모리 집약적, GPU/ASIC 공격에 대한 저항력
- **ML-KEM-768**: NIST FIPS 203 포스트양자 KEM — 미래 양자 컴퓨터로부터 보호
- **HKDF**: 두 소스를 결합; **어느 하나의** 소스가 안전하면 보안이 유지됨
- **디스크의 키스토어**: `[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **제로화**: 모든 키 자료는 소멸 시 제로화됨(`zeroize` 크레이트)

### 암호화

- 청크당 랜덤 12바이트 nonce를 사용하는 **AES-256-GCM**
- **AAD**(Additional Authenticated Data): 청크의 SHA-256 해시 — 암호문을 콘텐츠 ID에 바인딩
- 암호화된 데이터가 저장되고, nonce는 매니페스트에 저장

### 시크릿 관리

Enigma는 여러 키 프로바이더 백엔드를 지원합니다. 설정에서 `key_provider`를 설정하세요:

| 프로바이더 | `key_provider` | 필요한 설정 | Feature flag |
|-----------|---------------|-----------|-------------|
| 로컬(기본) | `"local"` | `keyfile_path` + 패스프레이즈 | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

설정의 클라우드 자격 증명은 `enigma encrypt-cred <value>`로 암호화할 수 있습니다 — TOML에 붙여넣을 `enc:...` 토큰을 생성합니다.

추가 보안:
- `enigma.toml`의 파일 권한
- 환경 변수(`ENIGMA_PASSPHRASE`, AWS 환경 변수 등)
- 키 파일 자체가 패스프레이즈로 암호화됨

## 빠른 시작

### 빌드

```bash
# 사전 요구사항: Rust 1.85+, protoc(tonic/prost용)
cargo build --release --workspace

# 선택적 feature 포함
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

# 바이너리 위치
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # S3 게이트웨이
```

### CLI 사용법

```bash
# 초기화(설정 + 암호화된 키 파일 생성)
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# 디렉토리 백업
enigma --passphrase "my-secret" backup /path/to/data

# 백업 목록
enigma list

# 무결성 검증
enigma --passphrase "my-secret" verify <backup-id>

# 복원(전체)
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# 선택적 복원
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # 접두사 필터
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # glob 필터
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # 파일 목록만

# 가비지 컬렉션
enigma gc --dry-run    # 고아 청크 목록
enigma gc              # 고아 청크 삭제

# 설정용 자격 증명 암호화
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

# 상태 / 설정 표시
enigma status
enigma config
```

### S3 게이트웨이(단일 노드)

```bash
# 프록시 시작
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# 아무 S3 클라이언트 사용
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## 설정

### 전체 참조(`enigma.toml`)

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # azure-keyvault용
# gcp_project_id = "my-project"                     # gcp-secretmanager용
# aws_region = "us-east-1"                          # aws-secretsmanager용
# secret_prefix = "enigma-key"                      # vault 시크릿 이름 접두사

# 청킹 — 하나 선택:
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 MB(기본)

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 MB

# 압축(선택적, 기본적으로 비활성화)
[enigma.compression]
enabled = false                          # true로 설정하여 zstd 활성화
level = 3                                # zstd 레벨 1-22(기본: 3)

# S3 프록시(enigma-proxy 전용)
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"
# tls_cert = "/path/to/cert.pem"         # HTTPS 활성화(feature: tls)
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # Prometheus 엔드포인트(feature: metrics)

# 스토리지 프로바이더 — 필요한 만큼 추가
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # 다음도 허용: "minio", "rustfs", "garage"
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
bucket = "enigma-container"              # 컨테이너 이름
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
bucket = "/data/enigma-local"            # 로컬 디렉토리 경로
weight = 1

# Raft(선택적, 다중 노드 HA용)
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

### 프로바이더 유형

| 유형 | 값 | 비고 |
|------|-----|------|
| 로컬 파일 시스템 | `Local` | `bucket` = 디렉토리 경로 |
| AWS S3 | `S3` | AWS SDK 기본 자격 증명 체인 사용 |
| S3 호환 | `S3Compatible`, `minio`, `rustfs`, `garage` | `endpoint_url`, `path_style = true` 필요 |
| Azure Blob Storage | `Azure` | `bucket` = 컨테이너 이름 |
| Google Cloud Storage | `Gcs` | Application Default Credentials 사용 |

### 환경 변수

| 변수 | 설명 |
|------|------|
| `ENIGMA_PASSPHRASE` | 키 암호화용 패스프레이즈(대화형 프롬프트 회피) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | AWS 자격 증명(S3 프로바이더용) |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | Azure 자격 증명 |
| `GOOGLE_APPLICATION_CREDENTIALS` | GCP 서비스 계정 JSON 경로 |
| `AWS_REGION` | Secrets Manager 키 프로바이더용 AWS 리전 |
| `RUST_LOG` | 로그 레벨 필터(예: `enigma=info,tower=warn`) |

## S3 API 호환성

| 작업 | 지원 |
|------|------|
| CreateBucket | 예 |
| DeleteBucket | 예(비어 있어야 함) |
| HeadBucket | 예 |
| ListBuckets | 예 |
| PutObject | 예 |
| GetObject | 예 |
| HeadObject | 예 |
| DeleteObject | 예 |
| ListObjectsV2 | 예(prefix, delimiter, max-keys, continuation-token) |
| CreateMultipartUpload | 예 |
| UploadPart | 예 |
| CompleteMultipartUpload | 예 |
| AbortMultipartUpload | 예 |

## 테스트

### 단위 및 통합 테스트(49개 이상)

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

### Vault 테스트(실제 자격 증명 필요)

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

### 테스트 커버리지

| 모듈 | 테스트 내용 |
|------|-----------|
| `chunk::cdc` | 빈 파일, 작은 파일(단일 청크), 큰 파일(다중 청크), 결정적 해시 |
| `chunk::fixed` | 빈 파일, 정확한 배수, 나머지 처리 |
| `compression` | 압축/해제 왕복, 빈 데이터 |
| `config` | TOML 직렬화 왕복, 파일 누락 오류 |
| `config::credentials` | 암호화/복호화 왕복, 평문 패스스루 |
| `crypto` | 암호화/복호화 왕복(원시 + 청크), 잘못된 키 거부, 잘못된 AAD 거부, 고유 nonce |
| `dedup` | 결정적 해싱, 다른 데이터 = 다른 해시, 중복 감지 |
| `distributor` | 라운드로빈 순환, 가중 분산, 프로바이더 검색 |
| `manifest::schema` | 테이블 생성, 마이그레이션 멱등성 |
| `manifest::queries` | 전체 백업 흐름, 목록 순서, 청크 참조 카운팅, 로그 |
| `types` | ChunkHash hex 왕복, 스토리지 키 형식, KeyMaterial 제로화, ProviderType 파싱 |
| `keys::local` | 키 파일 생성/열기, 잘못된 패스프레이즈, ML-KEM 크기, 하이브리드 키 독립성, 순환 |
| `keys::vault` | Azure KV, GCP SM, AWS SM — 생성, 조회, 순환, 목록(통합) |
| `storage::local` | 연결 테스트, 업로드/다운로드 왕복, 매니페스트 왕복 |

### 성능(Apple M3 Pro, release 빌드)

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### 파이프라인 처리량

| 단계 | 1 MB | 4 MB | 16 MB |
|------|------|------|-------|
| SHA-256 해싱 | 340 MB/s | 318 MB/s | 339 MB/s |
| AES-256-GCM 암호화 | 135 MB/s | 135 MB/s | 137 MB/s |
| AES-256-GCM 복호화 | 137 MB/s | 135 MB/s | 137 MB/s |
| zstd 압축(랜덤) | 4224 MB/s | 2484 MB/s | 1830 MB/s |
| zstd 압축(텍스트) | 6762 MB/s | 6242 MB/s | — |

#### 청킹

| 엔진 | 4 MB 파일 | 16 MB 파일 | 64 MB 파일 |
|------|----------|-----------|-----------|
| CDC(목표 4 MB) | 271 MB/s | 227 MB/s | 266 MB/s |
| Fixed(4 MB) | 308 MB/s | 221 MB/s | 310 MB/s |

#### 전체 파이프라인(청크 -> 해시 -> 압축 -> 암호화)

| 입력 | 청크 | 처리량 |
|------|------|--------|
| 4 MB | 1 | 70 MB/s |
| 16 MB | 2-3 | 66 MB/s |
| 64 MB | 10-16 | 69 MB/s |

#### 키 파생(Argon2id + ML-KEM-768 + HKDF)

| 작업 | 시간 |
|------|------|
| 생성(keygen + 암호화) | 17 ms |
| 열기(복호화 + 파생) | 15 ms |

> 병목은 AES-256-GCM(~135 MB/s). SHA-256과 zstd는 훨씬 빠름.
> 클라우드 백엔드로의 네트워크 I/O가 프로덕션에서는 일반적으로 실제 병목.

### E2E 테스트

```bash
# 3개의 RustFS 인스턴스 실행 필요(Kind 클러스터 또는 docker-compose)
./tests/e2e_rustfs.sh
```

테스트: init -> 5개 파일 백업 -> 검증 -> 복원 -> 원본 대 복원 diff.

### CI 파이프라인

GitHub Actions가 `main`으로의 모든 push/PR에서 실행:
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## 배포

### Docker Compose(3노드 클러스터)

```bash
docker compose up -d
# 3개 enigma-proxy 노드(포트 8333-8335) + 3개 RustFS 백엔드(포트 19001-19003)

# 테스트
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes(StatefulSet)

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3개 enigma 파드(StatefulSet) + 3개 RustFS 디플로이먼트
# 포트 8333의 enigma-s3 ClusterIP 서비스를 통한 S3 접근
```

### 단일 바이너리

```bash
# CLI 모드(백업/복원)
enigma --config-dir /etc/enigma backup /data

# 게이트웨이 모드(S3 프록시)
enigma-proxy --config /etc/enigma/config.toml
```

## 로드맵

- [x] 시크릿용 Vault 통합(AWS Secrets Manager, Azure Key Vault, GCP Secret Manager)
- [x] Prometheus 메트릭 엔드포인트
- [x] S3 게이트웨이 TLS 지원
- [x] 설정의 암호화된 자격 증명
- [x] 고아 청크 가비지 컬렉션
- [x] 선택적 복원(path/glob 필터)
- [ ] 증분 백업(변경된 파일만)
- [ ] 대역폭 제한
- [ ] 웹 UI 대시보드
- [ ] 스냅샷 기반 Raft 복구
- [ ] 복제 대안으로서의 이레이저 코딩(Reed-Solomon)

## 라이선스

Source-Available([LICENSE](LICENSE) 참조)
