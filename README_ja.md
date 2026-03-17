[English](README.md) | [Français](README_fr.md) | [Español](README_es.md) | [Deutsch](README_de.md) | [Italiano](README_it.md) | [Português](README_pt.md) | [Nederlands](README_nl.md) | [Polski](README_pl.md) | [Русский](README_ru.md) | **日本語** | [中文](README_zh.md) | [العربية](README_ar.md) | [한국어](README_ko.md)

# Enigma

S3互換ゲートウェイとRaftベースの高可用性を備えたマルチクラウド暗号化バックアップツール。

Enigmaはデータを暗号化、チャンク分割、重複排除、オプションで圧縮し、複数のクラウドストレージバックエンドに分散します。S3互換APIを公開しているため、あらゆるS3クライアント（aws-cli、mc、rclone、SDK）が透過的に対話できます。

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## アーキテクチャ

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

### データパイプライン

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

ハッシュは常に**元の平文**に対して計算されるため、圧縮が有効かどうかに関係なく重複排除は同一に機能します。マニフェストの`size_compressed`列（NULL = 未圧縮）は、読み取りパスに解凍が必要かどうかを通知します — 完全に後方互換です。

### クレート

| クレート | 役割 |
|---------|------|
| **enigma-core** | チャンク分割（FastCDC / Fixed）、暗号化（AES-256-GCM）、重複排除（SHA-256）、圧縮（zstd）、ディストリビュータ、マニフェスト（SQLite）、設定（TOML） |
| **enigma-storage** | `StorageProvider`トレイト + 実装：Local、S3、S3互換、Azure Blob、GCS |
| **enigma-keys** | `KeyProvider`トレイト + ローカルハイブリッドポスト量子（Argon2id + ML-KEM-768）、Azure Key Vault、GCP Secret Manager、AWS Secrets Manager |
| **enigma-cli** | CLIバイナリ（`enigma`）— init、backup、restore、verify、list、status、config、gc、encrypt-cred |
| **enigma-s3** | s3s v0.11上に構築されたS3フロントエンド — PutObject、GetObject、HeadObject、DeleteObject、ListObjectsV2、buckets、multipart |
| **enigma-raft** | Raftコンセンサス（openraft v0.9 + tonic gRPC）— HAメタデータレプリケーション用のManifestDbをラップするステートマシン |
| **enigma-proxy** | S3ゲートウェイ + Raftを組み合わせたバイナリ — シングルノードまたはクラスタモード |

## 機能

- **エンドツーエンド暗号化** — AES-256-GCM、鍵はクライアントの外に出ない
- **ハイブリッドポスト量子鍵導出** — Argon2id + ML-KEM-768（FIPS 203）をHKDF-SHA256で結合
- **コンテンツ定義チャンキング** — 設定可能なターゲットサイズ（デフォルト4 MB）のFastCDCまたは固定サイズチャンク
- **SHA-256重複排除** — 同一チャンクはすべてのバックアップで一度だけ保存
- **オプションのzstd圧縮** — 暗号化前に適用、デフォルトで無効、後方互換
- **マルチクラウド分散** — プロバイダー間のラウンドロビンまたは重み付き分散
- **S3互換ゲートウェイ** — 完全なCRUD、マルチパートアップロード、prefix/delimiter付きListObjectsV2
- **Raft HA** — メタデータレプリケーション用の3ノードコンセンサス（データはバックエンドに直接送信）
- **シングルノードモード** — Raftなしで動作、プロバイダー未設定時はローカルストレージにフォールバック
- **Vault鍵プロバイダー** — Azure Key Vault、GCP Secret Manager、AWS Secrets Manager（フィーチャーフラグの後ろ）
- **TLS S3ゲートウェイ** — rustlsによるオプションのHTTPS（PEM cert/key）
- **Prometheusメトリクス** — 設定可能なポートの`/metrics`エンドポイント（`metrics`フィーチャーの後ろ）
- **暗号化された資格情報** — TOML設定内のAES-256-GCM暗号化シークレット（`enc:`プレフィックス）
- **ガベージコレクション** — `enigma gc`で孤立チャンクを検出・削除（`--dry-run`付き）
- **選択的リストア** — リストア時の`--path`、`--glob`、`--list`フィルター
- **監査証跡** — バックアップログとチャンク参照カウント付きSQLiteマニフェスト
- **鍵ローテーション** — 新しいハイブリッド鍵を生成、古い鍵はIDでアクセス可能

## セキュリティモデル

### 鍵導出

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id**: メモリハード、GPU/ASIC攻撃に耐性
- **ML-KEM-768**: NIST FIPS 203ポスト量子KEM — 将来の量子コンピュータから保護
- **HKDF**: 両方のソースを結合；**いずれかの**ソースが安全であればセキュリティが維持される
- **ディスク上のキーストア**: `[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **ゼロ化**: すべての鍵素材は破棄時にゼロ化される（`zeroize`クレート）

### 暗号化

- チャンクごとにランダムな12バイトnonceを持つ**AES-256-GCM**
- **AAD**（Additional Authenticated Data）: チャンクのSHA-256ハッシュ — 暗号文をコンテンツIDに紐付け
- 暗号化データが保存され、nonceはマニフェストに保存

### シークレット管理

Enigmaは複数の鍵プロバイダーバックエンドをサポートしています。設定で`key_provider`を設定してください:

| プロバイダー | `key_provider` | 必要な設定 | フィーチャーフラグ |
|-------------|---------------|-----------|-----------------|
| ローカル（デフォルト） | `"local"` | `keyfile_path` + パスフレーズ | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

設定内のクラウド資格情報は`enigma encrypt-cred <value>`で暗号化できます — TOMLに貼り付ける`enc:...`トークンを生成します。

追加のセキュリティ:
- `enigma.toml`のファイルパーミッション
- 環境変数（`ENIGMA_PASSPHRASE`、AWS環境変数など）
- 鍵ファイル自体がパスフレーズで暗号化

## クイックスタート

### ビルド

```bash
# 前提条件: Rust 1.85+、protoc（tonic/prost用）
cargo build --release --workspace

# オプションのフィーチャー付き
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

# バイナリの場所
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # S3ゲートウェイ
```

### CLI使用方法

```bash
# 初期化（設定 + 暗号化鍵ファイルを作成）
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# ディレクトリをバックアップ
enigma --passphrase "my-secret" backup /path/to/data

# バックアップ一覧
enigma list

# 整合性を検証
enigma --passphrase "my-secret" verify <backup-id>

# リストア（完全）
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# 選択的リストア
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # プレフィックスフィルター
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # globフィルター
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # ファイル一覧のみ

# ガベージコレクション
enigma gc --dry-run    # 孤立チャンクを一覧
enigma gc              # 孤立チャンクを削除

# 設定用の資格情報を暗号化
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

# ステータス / 設定を表示
enigma status
enigma config
```

### S3ゲートウェイ（シングルノード）

```bash
# プロキシを起動
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# 任意のS3クライアントを使用
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## 設定

### 完全なリファレンス（`enigma.toml`）

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # azure-keyvault用
# gcp_project_id = "my-project"                     # gcp-secretmanager用
# aws_region = "us-east-1"                          # aws-secretsmanager用
# secret_prefix = "enigma-key"                      # vaultシークレット名のプレフィックス

# チャンキング — いずれかを選択:
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 MB（デフォルト）

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 MB

# 圧縮（オプション、デフォルトで無効）
[enigma.compression]
enabled = false                          # trueに設定してzstdを有効化
level = 3                                # zstdレベル 1-22（デフォルト: 3）

# S3プロキシ（enigma-proxyのみ）
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"
# tls_cert = "/path/to/cert.pem"         # HTTPSを有効化（フィーチャー: tls）
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # Prometheusエンドポイント（フィーチャー: metrics）

# ストレージプロバイダー — 必要な数だけ追加
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # 以下も使用可能: "minio", "rustfs", "garage"
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
bucket = "enigma-container"              # コンテナ名
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
bucket = "/data/enigma-local"            # ローカルディレクトリパス
weight = 1

# Raft（オプション、マルチノードHA用）
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

### プロバイダータイプ

| タイプ | 値 | 備考 |
|-------|-----|------|
| ローカルファイルシステム | `Local` | `bucket` = ディレクトリパス |
| AWS S3 | `S3` | AWS SDKのデフォルト認証チェーンを使用 |
| S3互換 | `S3Compatible`, `minio`, `rustfs`, `garage` | `endpoint_url`、`path_style = true`が必要 |
| Azure Blob Storage | `Azure` | `bucket` = コンテナ名 |
| Google Cloud Storage | `Gcs` | Application Default Credentialsを使用 |

### 環境変数

| 変数 | 説明 |
|------|------|
| `ENIGMA_PASSPHRASE` | 鍵暗号化用パスフレーズ（対話的プロンプトを回避） |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | AWS資格情報（S3プロバイダー用） |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | Azure資格情報 |
| `GOOGLE_APPLICATION_CREDENTIALS` | GCPサービスアカウントJSONへのパス |
| `AWS_REGION` | Secrets Manager鍵プロバイダー用AWSリージョン |
| `RUST_LOG` | ログレベルフィルター（例: `enigma=info,tower=warn`） |

## S3 API互換性

| オペレーション | サポート |
|--------------|---------|
| CreateBucket | はい |
| DeleteBucket | はい（空である必要あり） |
| HeadBucket | はい |
| ListBuckets | はい |
| PutObject | はい |
| GetObject | はい |
| HeadObject | はい |
| DeleteObject | はい |
| ListObjectsV2 | はい（prefix、delimiter、max-keys、continuation-token） |
| CreateMultipartUpload | はい |
| UploadPart | はい |
| CompleteMultipartUpload | はい |
| AbortMultipartUpload | はい |

## テスト

### ユニットテスト & 統合テスト（49以上のテスト）

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

### Vaultテスト（実際の資格情報が必要）

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

### テストカバレッジ

| モジュール | テスト内容 |
|-----------|----------|
| `chunk::cdc` | 空ファイル、小ファイル（単一チャンク）、大ファイル（マルチチャンク）、決定論的ハッシュ |
| `chunk::fixed` | 空ファイル、正確な倍数、余り処理 |
| `compression` | 圧縮/解凍の往復、空データ |
| `config` | TOMLシリアライズの往復、ファイル不在エラー |
| `config::credentials` | 暗号化/復号の往復、平文パススルー |
| `crypto` | 暗号化/復号の往復（raw + チャンク）、不正な鍵の拒否、不正なAADの拒否、ユニークなnonce |
| `dedup` | 決定論的ハッシング、異なるデータ = 異なるハッシュ、重複検出 |
| `distributor` | ラウンドロビンサイクル、重み付き分散、プロバイダー検索 |
| `manifest::schema` | テーブル作成、マイグレーションの冪等性 |
| `manifest::queries` | 完全なバックアップフロー、リスト順序、チャンク参照カウント、ログ |
| `types` | ChunkHash hexの往復、ストレージキー形式、KeyMaterialゼロ化、ProviderTypeパース |
| `keys::local` | 鍵ファイルの作成/オープン、不正なパスフレーズ、ML-KEMサイズ、ハイブリッド鍵の独立性、ローテーション |
| `keys::vault` | Azure KV、GCP SM、AWS SM — 作成、取得、ローテーション、一覧（統合） |
| `storage::local` | 接続テスト、アップロード/ダウンロードの往復、マニフェストの往復 |

### パフォーマンス（Apple M3 Pro、リリースビルド）

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### パイプラインスループット

| ステージ | 1 MB | 4 MB | 16 MB |
|---------|------|------|-------|
| SHA-256ハッシング | 340 MB/s | 318 MB/s | 339 MB/s |
| AES-256-GCM暗号化 | 135 MB/s | 135 MB/s | 137 MB/s |
| AES-256-GCM復号 | 137 MB/s | 135 MB/s | 137 MB/s |
| zstd圧縮（ランダム） | 4224 MB/s | 2484 MB/s | 1830 MB/s |
| zstd圧縮（テキスト） | 6762 MB/s | 6242 MB/s | — |

#### チャンキング

| エンジン | 4 MBファイル | 16 MBファイル | 64 MBファイル |
|---------|------------|-------------|-------------|
| CDC（ターゲット4 MB） | 271 MB/s | 227 MB/s | 266 MB/s |
| Fixed（4 MB） | 308 MB/s | 221 MB/s | 310 MB/s |

#### フルパイプライン（チャンク -> ハッシュ -> 圧縮 -> 暗号化）

| 入力 | チャンク数 | スループット |
|------|----------|------------|
| 4 MB | 1 | 70 MB/s |
| 16 MB | 2-3 | 66 MB/s |
| 64 MB | 10-16 | 69 MB/s |

#### 鍵導出（Argon2id + ML-KEM-768 + HKDF）

| オペレーション | 時間 |
|-------------|------|
| 作成（keygen + 暗号化） | 17 ms |
| オープン（復号 + 導出） | 15 ms |

> ボトルネックはAES-256-GCM（~135 MB/s）。SHA-256とzstdははるかに高速。
> クラウドバックエンドへのネットワークI/Oが本番環境では通常の実際のボトルネック。

### E2Eテスト

```bash
# 3つのRustFSインスタンスが実行中である必要あり（Kindクラスタまたはdocker-compose）
./tests/e2e_rustfs.sh
```

テスト: init -> 5ファイルのバックアップ -> 検証 -> リストア -> オリジナルとリストア済みのdiff。

### CIパイプライン

GitHub Actionsは`main`へのプッシュ/PRごとに実行:
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## デプロイ

### Docker Compose（3ノードクラスタ）

```bash
docker compose up -d
# 3つのenigma-proxyノード（ポート8333-8335）+ 3つのRustFSバックエンド（ポート19001-19003）

# テスト
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes（StatefulSet）

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3つのenigmaポッド（StatefulSet）+ 3つのRustFSデプロイメント
# ポート8333のenigma-s3 ClusterIPサービス経由でS3アクセス
```

### 単一バイナリ

```bash
# CLIモード（バックアップ/リストア）
enigma --config-dir /etc/enigma backup /data

# ゲートウェイモード（S3プロキシ）
enigma-proxy --config /etc/enigma/config.toml
```

## ロードマップ

- [x] シークレット用Vault統合（AWS Secrets Manager、Azure Key Vault、GCP Secret Manager）
- [x] Prometheusメトリクスエンドポイント
- [x] S3ゲートウェイのTLSサポート
- [x] 設定内の暗号化された資格情報
- [x] 孤立チャンクのガベージコレクション
- [x] 選択的リストア（path/globフィルター）
- [ ] 増分バックアップ（変更されたファイルのみ）
- [ ] 帯域幅制限
- [ ] Web UIダッシュボード
- [ ] スナップショットベースのRaftリカバリ
- [ ] レプリケーションの代替としてのイレイジャーコーディング（Reed-Solomon）

## ライセンス

Source-Available（[LICENSE](LICENSE)を参照）
