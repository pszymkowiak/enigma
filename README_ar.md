[English](README.md) | [Français](README_fr.md) | [Español](README_es.md) | [Deutsch](README_de.md) | [Italiano](README_it.md) | [Português](README_pt.md) | [Nederlands](README_nl.md) | [Polski](README_pl.md) | [Русский](README_ru.md) | [日本語](README_ja.md) | [中文](README_zh.md) | **العربية** | [한국어](README_ko.md)

# Enigma

أداة نسخ احتياطي مشفرة متعددة السحابة مع بوابة متوافقة مع S3 وتوفر عالٍ قائم على Raft.

يقوم Enigma بتشفير البيانات وتقسيمها وإزالة التكرار وضغطها اختيارياً وتوزيعها عبر عدة خلفيات تخزين سحابية. يوفر واجهة برمجة تطبيقات متوافقة مع S3 بحيث يمكن لأي عميل S3 (aws-cli، mc، rclone، SDKs) التفاعل معه بشفافية.

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## الهندسة المعمارية

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

### خط أنابيب البيانات

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

يتم حساب التجزئة دائماً على **النص الأصلي**، لذا تعمل إزالة التكرار بشكل مطابق سواء كان الضغط مفعلاً أم لا. يشير عمود `size_compressed` في السجل (NULL = غير مضغوط) لمسار القراءة ما إذا كان فك الضغط مطلوباً — متوافق تماماً مع الإصدارات السابقة.

### الحزم (Crates)

| الحزمة | الدور |
|--------|-------|
| **enigma-core** | التقسيم (FastCDC / Fixed)، التشفير (AES-256-GCM)، إزالة التكرار (SHA-256)، الضغط (zstd)، الموزع، السجل (SQLite)، الإعداد (TOML) |
| **enigma-storage** | سمة `StorageProvider` + التطبيقات: Local، S3، متوافق مع S3، Azure Blob، GCS |
| **enigma-keys** | سمة `KeyProvider` + محلي هجين ما بعد الكم (Argon2id + ML-KEM-768)، Azure Key Vault، GCP Secret Manager، AWS Secrets Manager |
| **enigma-cli** | ملف CLI التنفيذي (`enigma`) — init، backup، restore، verify، list، status، config، gc، encrypt-cred |
| **enigma-s3** | واجهة S3 الأمامية مبنية على s3s v0.11 — PutObject، GetObject، HeadObject، DeleteObject، ListObjectsV2، buckets، multipart |
| **enigma-raft** | إجماع Raft (openraft v0.9 + tonic gRPC) — آلة حالة تغلف ManifestDb لتكرار البيانات الوصفية عالي التوفر |
| **enigma-proxy** | ملف تنفيذي يجمع بين بوابة S3 + Raft — وضع العقدة الواحدة أو المجموعة |

## الميزات

- **تشفير شامل** — AES-256-GCM، المفاتيح لا تغادر العميل أبداً
- **اشتقاق مفتاح هجين ما بعد الكم** — Argon2id + ML-KEM-768 (FIPS 203) مدمجان عبر HKDF-SHA256
- **تقسيم محدد بالمحتوى** — FastCDC مع حجم هدف قابل للتكوين (افتراضي 4 ميجابايت) أو أجزاء بحجم ثابت
- **إزالة تكرار SHA-256** — الأجزاء المتطابقة تُخزن مرة واحدة فقط عبر جميع النسخ الاحتياطية
- **ضغط zstd اختياري** — يُطبق قبل التشفير، معطل افتراضياً، متوافق مع الإصدارات السابقة
- **توزيع متعدد السحابة** — دوري أو توزيع مرجح بين المزودين
- **بوابة متوافقة مع S3** — CRUD كامل، رفع متعدد الأجزاء، ListObjectsV2 مع prefix/delimiter
- **Raft HA** — إجماع 3 عقد لتكرار البيانات الوصفية (البيانات تذهب مباشرة إلى الخلفيات)
- **وضع العقدة الواحدة** — يعمل بدون Raft، تراجع للتخزين المحلي إذا لم يتم تكوين مزودين
- **مزودو مفاتيح Vault** — Azure Key Vault، GCP Secret Manager، AWS Secrets Manager (خلف أعلام الميزات)
- **بوابة S3 بـ TLS** — HTTPS اختياري مع rustls (شهادة/مفتاح PEM)
- **مقاييس Prometheus** — نقطة نهاية `/metrics` على منفذ قابل للتكوين (خلف ميزة `metrics`)
- **بيانات اعتماد مشفرة** — أسرار مشفرة بـ AES-256-GCM في إعداد TOML (بادئة `enc:`)
- **جمع القمامة** — `enigma gc` للبحث عن الأجزاء اليتيمة وحذفها (مع `--dry-run`)
- **استعادة انتقائية** — مرشحات `--path`، `--glob`، `--list` عند الاستعادة
- **سجل مراجعة** — سجل SQLite مع سجلات النسخ الاحتياطي وعد مراجع الأجزاء
- **تدوير المفاتيح** — إنشاء مفاتيح هجينة جديدة، المفاتيح القديمة تبقى قابلة للوصول بالمعرف

## نموذج الأمان

### اشتقاق المفتاح

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id**: مقاوم للذاكرة، مقاوم لهجمات GPU/ASIC
- **ML-KEM-768**: NIST FIPS 203 KEM ما بعد الكم — يحمي من أجهزة الكمبيوتر الكمية المستقبلية
- **HKDF**: يجمع كلا المصدرين؛ يبقى الأمان محفوظاً إذا كان **أي من** المصدرين غير مخترق
- **مخزن المفاتيح على القرص**: `[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **التصفير**: جميع مواد المفاتيح تُصفر عند التدمير (حزمة `zeroize`)

### التشفير

- **AES-256-GCM** لكل جزء مع nonce عشوائي بطول 12 بايت
- **AAD** (بيانات مصادقة إضافية): تجزئة SHA-256 للجزء — يربط النص المشفر بهوية محتواه
- البيانات المشفرة تُخزن؛ الـ nonce يُخزن في السجل

### إدارة الأسرار

يدعم Enigma عدة خلفيات لمزودي المفاتيح. عيّن `key_provider` في الإعداد:

| المزود | `key_provider` | الإعداد المطلوب | علم الميزة |
|--------|---------------|-----------------|-----------|
| محلي (افتراضي) | `"local"` | `keyfile_path` + كلمة المرور | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

يمكن تشفير بيانات الاعتماد السحابية في الإعداد باستخدام `enigma encrypt-cred <value>` — ينتج رمز `enc:...` للصقه في TOML.

أمان إضافي:
- أذونات الملف على `enigma.toml`
- متغيرات البيئة (`ENIGMA_PASSPHRASE`، متغيرات AWS، إلخ)
- ملف المفاتيح نفسه مشفر بكلمة المرور

## البداية السريعة

### البناء

```bash
# المتطلبات الأساسية: Rust 1.85+، protoc (لـ tonic/prost)
cargo build --release --workspace

# مع ميزات اختيارية
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

# مواقع الملفات التنفيذية
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # بوابة S3
```

### استخدام CLI

```bash
# التهيئة (إنشاء الإعداد + ملف المفاتيح المشفر)
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# نسخ احتياطي لمجلد
enigma --passphrase "my-secret" backup /path/to/data

# عرض النسخ الاحتياطية
enigma list

# التحقق من السلامة
enigma --passphrase "my-secret" verify <backup-id>

# استعادة (كاملة)
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# استعادة انتقائية
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # مرشح البادئة
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # مرشح glob
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # عرض الملفات فقط

# جمع القمامة
enigma gc --dry-run    # عرض الأجزاء اليتيمة
enigma gc              # حذف الأجزاء اليتيمة

# تشفير بيانات اعتماد للإعداد
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

# عرض الحالة / الإعداد
enigma status
enigma config
```

### بوابة S3 (عقدة واحدة)

```bash
# تشغيل الوكيل
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# استخدام أي عميل S3
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## الإعداد

### المرجع الكامل (`enigma.toml`)

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # لـ azure-keyvault
# gcp_project_id = "my-project"                     # لـ gcp-secretmanager
# aws_region = "us-east-1"                          # لـ aws-secretsmanager
# secret_prefix = "enigma-key"                      # بادئة لأسماء أسرار vault

# التقسيم — اختر واحداً:
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 ميجابايت (افتراضي)

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 ميجابايت

# الضغط (اختياري، معطل افتراضياً)
[enigma.compression]
enabled = false                          # عيّنه إلى true لتفعيل zstd
level = 3                                # مستوى zstd 1-22 (افتراضي: 3)

# وكيل S3 (enigma-proxy فقط)
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"
# tls_cert = "/path/to/cert.pem"         # يفعل HTTPS (ميزة: tls)
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # نقطة نهاية Prometheus (ميزة: metrics)

# مزودو التخزين — أضف بقدر ما تحتاج
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # يقبل أيضاً: "minio"، "rustfs"، "garage"
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
bucket = "enigma-container"              # اسم الحاوية
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
bucket = "/data/enigma-local"            # مسار المجلد المحلي
weight = 1

# Raft (اختياري، للتوفر العالي متعدد العقد)
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

### أنواع المزودين

| النوع | القيمة/القيم | ملاحظات |
|-------|-------------|---------|
| نظام ملفات محلي | `Local` | `bucket` = مسار المجلد |
| AWS S3 | `S3` | يستخدم سلسلة بيانات اعتماد AWS SDK الافتراضية |
| متوافق مع S3 | `S3Compatible`، `minio`، `rustfs`، `garage` | يتطلب `endpoint_url`، `path_style = true` |
| Azure Blob Storage | `Azure` | `bucket` = اسم الحاوية |
| Google Cloud Storage | `Gcs` | يستخدم Application Default Credentials |

### متغيرات البيئة

| المتغير | الوصف |
|---------|-------|
| `ENIGMA_PASSPHRASE` | كلمة المرور لتشفير المفاتيح (يتجنب المطالبة التفاعلية) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | بيانات اعتماد AWS (لمزود S3) |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | بيانات اعتماد Azure |
| `GOOGLE_APPLICATION_CREDENTIALS` | مسار ملف JSON لحساب خدمة GCP |
| `AWS_REGION` | منطقة AWS لمزود مفاتيح Secrets Manager |
| `RUST_LOG` | مرشح مستوى السجل (مثال: `enigma=info,tower=warn`) |

## توافق واجهة S3

| العملية | مدعومة |
|---------|--------|
| CreateBucket | نعم |
| DeleteBucket | نعم (يجب أن يكون فارغاً) |
| HeadBucket | نعم |
| ListBuckets | نعم |
| PutObject | نعم |
| GetObject | نعم |
| HeadObject | نعم |
| DeleteObject | نعم |
| ListObjectsV2 | نعم (prefix، delimiter، max-keys، continuation-token) |
| CreateMultipartUpload | نعم |
| UploadPart | نعم |
| CompleteMultipartUpload | نعم |
| AbortMultipartUpload | نعم |

## الاختبارات

### اختبارات الوحدة والتكامل (49+ اختبار)

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

### اختبارات Vault (تتطلب بيانات اعتماد حقيقية)

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

### تغطية الاختبارات

| الوحدة | ما يتم اختباره |
|--------|---------------|
| `chunk::cdc` | ملف فارغ، ملف صغير (جزء واحد)، ملف كبير (أجزاء متعددة)، تجزئات حتمية |
| `chunk::fixed` | ملف فارغ، مضاعف دقيق، معالجة الباقي |
| `compression` | ذهاب وإياب ضغط/فك ضغط، بيانات فارغة |
| `config` | ذهاب وإياب تسلسل TOML، خطأ ملف مفقود |
| `config::credentials` | ذهاب وإياب تشفير/فك تشفير، تمرير النص العادي |
| `crypto` | ذهاب وإياب تشفير/فك تشفير (خام + جزء)، رفض مفتاح خاطئ، رفض AAD خاطئ، nonces فريدة |
| `dedup` | تجزئة حتمية، بيانات مختلفة = تجزئات مختلفة، اكتشاف التكرار |
| `distributor` | دورة round-robin، توزيع مرجح، البحث عن مزود |
| `manifest::schema` | إنشاء الجداول، عدم تأثر الترحيل بالتكرار |
| `manifest::queries` | تدفق النسخ الاحتياطي الكامل، ترتيب القائمة، عد مراجع الأجزاء، السجلات |
| `types` | ذهاب وإياب hex لـ ChunkHash، تنسيق مفتاح التخزين، تصفير KeyMaterial، تحليل ProviderType |
| `keys::local` | إنشاء/فتح ملف المفاتيح، كلمة مرور خاطئة، أحجام ML-KEM، استقلالية المفاتيح الهجينة، التدوير |
| `keys::vault` | Azure KV، GCP SM، AWS SM — إنشاء، جلب، تدوير، عرض (تكامل) |
| `storage::local` | اختبار الاتصال، ذهاب وإياب رفع/تنزيل، ذهاب وإياب السجل |

### الأداء (Apple M3 Pro، بناء release)

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### إنتاجية خط الأنابيب

| المرحلة | 1 ميجابايت | 4 ميجابايت | 16 ميجابايت |
|---------|-----------|-----------|------------|
| تجزئة SHA-256 | 340 م.ب/ث | 318 م.ب/ث | 339 م.ب/ث |
| تشفير AES-256-GCM | 135 م.ب/ث | 135 م.ب/ث | 137 م.ب/ث |
| فك تشفير AES-256-GCM | 137 م.ب/ث | 135 م.ب/ث | 137 م.ب/ث |
| ضغط zstd (عشوائي) | 4224 م.ب/ث | 2484 م.ب/ث | 1830 م.ب/ث |
| ضغط zstd (نص) | 6762 م.ب/ث | 6242 م.ب/ث | — |

#### التقسيم

| المحرك | ملف 4 م.ب | ملف 16 م.ب | ملف 64 م.ب |
|--------|----------|-----------|-----------|
| CDC (هدف 4 م.ب) | 271 م.ب/ث | 227 م.ب/ث | 266 م.ب/ث |
| Fixed (4 م.ب) | 308 م.ب/ث | 221 م.ب/ث | 310 م.ب/ث |

#### خط الأنابيب الكامل (تقسيم -> تجزئة -> ضغط -> تشفير)

| الإدخال | الأجزاء | الإنتاجية |
|---------|---------|----------|
| 4 م.ب | 1 | 70 م.ب/ث |
| 16 م.ب | 2-3 | 66 م.ب/ث |
| 64 م.ب | 10-16 | 69 م.ب/ث |

#### اشتقاق المفتاح (Argon2id + ML-KEM-768 + HKDF)

| العملية | الوقت |
|---------|-------|
| الإنشاء (keygen + تشفير) | 17 مللي ثانية |
| الفتح (فك تشفير + اشتقاق) | 15 مللي ثانية |

> عنق الزجاجة هو AES-256-GCM (~135 م.ب/ث). SHA-256 وzstd أسرع بكثير.
> عمليات الإدخال/الإخراج عبر الشبكة إلى الخلفيات السحابية هي عادةً عنق الزجاجة الحقيقي في الإنتاج.

### اختبار E2E

```bash
# يتطلب 3 مثيلات RustFS قيد التشغيل (مجموعة Kind أو docker-compose)
./tests/e2e_rustfs.sh
```

الاختبارات: init -> نسخ احتياطي 5 ملفات -> التحقق -> الاستعادة -> diff الأصل مقابل المستعاد.

### خط أنابيب CI

يعمل GitHub Actions عند كل push/PR إلى `main`:
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## النشر

### Docker Compose (مجموعة من 3 عقد)

```bash
docker compose up -d
# 3 عقد enigma-proxy (منافذ 8333-8335) + 3 خلفيات RustFS (منافذ 19001-19003)

# اختبار
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes (StatefulSet)

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3 حاويات enigma (StatefulSet) + 3 عمليات نشر RustFS
# الوصول إلى S3 عبر خدمة ClusterIP enigma-s3 على المنفذ 8333
```

### ملف تنفيذي واحد

```bash
# وضع CLI (نسخ احتياطي/استعادة)
enigma --config-dir /etc/enigma backup /data

# وضع البوابة (وكيل S3)
enigma-proxy --config /etc/enigma/config.toml
```

## خارطة الطريق

- [x] تكامل Vault للأسرار (AWS Secrets Manager، Azure Key Vault، GCP Secret Manager)
- [x] نقطة نهاية مقاييس Prometheus
- [x] دعم TLS لبوابة S3
- [x] بيانات اعتماد مشفرة في الإعداد
- [x] جمع القمامة للأجزاء اليتيمة
- [x] استعادة انتقائية (مرشحات path/glob)
- [ ] نسخ احتياطي تزايدي (الملفات المتغيرة فقط)
- [ ] تحديد عرض النطاق الترددي
- [ ] لوحة تحكم Web UI
- [ ] استرداد Raft المبني على اللقطات
- [ ] ترميز المحو (Reed-Solomon) كبديل للتكرار

## الترخيص

Source-Available (انظر [LICENSE](LICENSE))
