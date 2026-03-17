[English](README.md) | **Français** | [Español](README_es.md) | [Deutsch](README_de.md) | [Italiano](README_it.md) | [Português](README_pt.md) | [Nederlands](README_nl.md) | [Polski](README_pl.md) | [Русский](README_ru.md) | [日本語](README_ja.md) | [中文](README_zh.md) | [العربية](README_ar.md) | [한국어](README_ko.md)

# Enigma

Outil de sauvegarde chiffrée multi-cloud avec passerelle S3 compatible et haute disponibilité basée sur Raft.

Enigma chiffre, découpe, déduplique, compresse optionnellement et distribue les données sur plusieurs backends de stockage cloud. Il expose une API compatible S3 pour que tout client S3 (aws-cli, mc, rclone, SDKs) puisse interagir avec lui de manière transparente.

[![CI](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml/badge.svg)](https://github.com/pszymkowiak/enigma/actions/workflows/ci.yml)

## Architecture

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

### Pipeline de données

```
PUT:  Data -> Chunk (CDC/Fixed) -> SHA-256(plaintext) -> [zstd compress] -> AES-256-GCM encrypt -> Upload
GET:  Download -> AES-256-GCM decrypt -> [zstd decompress if compressed] -> SHA-256 verify -> Reassemble
```

Le hash est toujours calculé sur le **texte clair original**, donc la déduplication fonctionne de manière identique que la compression soit activée ou non. La colonne `size_compressed` dans le manifeste (NULL = non compressé) indique au chemin de lecture si la décompression est nécessaire — entièrement rétrocompatible.

### Crates

| Crate | Rôle |
|-------|------|
| **enigma-core** | Découpage (FastCDC / Fixed), crypto (AES-256-GCM), dédup (SHA-256), compression (zstd), distributeur, manifeste (SQLite), config (TOML) |
| **enigma-storage** | Trait `StorageProvider` + implémentations : Local, S3, S3-compatible, Azure Blob, GCS |
| **enigma-keys** | Trait `KeyProvider` + local hybride post-quantique (Argon2id + ML-KEM-768), Azure Key Vault, GCP Secret Manager, AWS Secrets Manager |
| **enigma-cli** | Binaire CLI (`enigma`) — init, backup, restore, verify, list, status, config, gc, encrypt-cred |
| **enigma-s3** | Frontend S3 construit sur s3s v0.11 — PutObject, GetObject, HeadObject, DeleteObject, ListObjectsV2, buckets, multipart |
| **enigma-raft** | Consensus Raft (openraft v0.9 + tonic gRPC) — machine à états encapsulant ManifestDb pour la réplication HA des métadonnées |
| **enigma-proxy** | Binaire combinant passerelle S3 + Raft — mode nœud unique ou cluster |

## Fonctionnalités

- **Chiffrement de bout en bout** — AES-256-GCM, les clés ne quittent jamais le client
- **Dérivation de clé hybride post-quantique** — Argon2id + ML-KEM-768 (FIPS 203) combinés via HKDF-SHA256
- **Découpage par contenu** — FastCDC avec taille cible configurable (par défaut 4 Mo) ou blocs de taille fixe
- **Déduplication SHA-256** — les blocs identiques ne sont stockés qu'une seule fois pour toutes les sauvegardes
- **Compression zstd optionnelle** — appliquée avant le chiffrement, désactivée par défaut, rétrocompatible
- **Distribution multi-cloud** — round-robin ou distribution pondérée entre les fournisseurs
- **Passerelle compatible S3** — CRUD complet, uploads multipart, ListObjectsV2 avec prefix/delimiter
- **Raft HA** — consensus à 3 nœuds pour la réplication des métadonnées (les données vont directement aux backends)
- **Mode nœud unique** — fonctionne sans Raft, repli sur stockage local si aucun fournisseur configuré
- **Fournisseurs de clés Vault** — Azure Key Vault, GCP Secret Manager, AWS Secrets Manager (derrière des feature flags)
- **Passerelle S3 TLS** — HTTPS optionnel avec rustls (cert/clé PEM)
- **Métriques Prometheus** — endpoint `/metrics` sur port configurable (derrière la feature `metrics`)
- **Identifiants chiffrés** — secrets chiffrés AES-256-GCM dans la config TOML (préfixe `enc:`)
- **Ramasse-miettes** — `enigma gc` pour trouver et supprimer les blocs orphelins (avec `--dry-run`)
- **Restauration sélective** — filtres `--path`, `--glob`, `--list` à la restauration
- **Piste d'audit** — manifeste SQLite avec journaux de sauvegarde et comptage de références des blocs
- **Rotation des clés** — génération de nouvelles clés hybrides, les anciennes clés restent accessibles par ID

## Modèle de sécurité

### Dérivation de clé

```
Passphrase ──> Argon2id(salt) ──> 32-byte symmetric key ─┐
                                                          ├─> HKDF-SHA256 ──> Final 256-bit key
ML-KEM-768 encapsulate(ek) ──> 32-byte shared secret ────┘
                                   info = "enigma-hybrid-v1"
```

- **Argon2id** : résistant à la mémoire, résistant aux attaques GPU/ASIC
- **ML-KEM-768** : NIST FIPS 203 post-quantique KEM — protège contre les futurs ordinateurs quantiques
- **HKDF** : combine les deux sources ; la sécurité tient si **l'une ou l'autre** source n'est pas compromise
- **Keystore sur disque** : `[salt 32B] + [nonce 12B] + [AES-256-GCM ciphertext of JSON keystore]`
- **Zéroisation** : tout le matériel de clé est zéroïsé à la destruction (crate `zeroize`)

### Chiffrement

- **AES-256-GCM** par bloc avec nonce aléatoire de 12 octets
- **AAD** (Additional Authenticated Data) : hash SHA-256 du bloc — lie le texte chiffré à l'identité de son contenu
- Les données chiffrées sont stockées ; le nonce est stocké dans le manifeste

### Gestion des secrets

Enigma prend en charge plusieurs backends de fournisseurs de clés. Configurez `key_provider` dans la config :

| Fournisseur | `key_provider` | Config requise | Feature flag |
|-------------|---------------|----------------|-------------|
| Local (défaut) | `"local"` | `keyfile_path` + passphrase | — |
| Azure Key Vault | `"azure-keyvault"` | `vault_url` | `--features azure-keyvault` |
| GCP Secret Manager | `"gcp-secretmanager"` | `gcp_project_id` | `--features gcp-secretmanager` |
| AWS Secrets Manager | `"aws-secretsmanager"` | `aws_region` | `--features aws-secretsmanager` |

Les identifiants cloud dans la config peuvent être chiffrés avec `enigma encrypt-cred <value>` — produit un token `enc:...` à coller dans le TOML.

Sécurité supplémentaire :
- Permissions de fichier sur `enigma.toml`
- Variables d'environnement (`ENIGMA_PASSPHRASE`, variables AWS, etc.)
- Le fichier de clés est lui-même chiffré avec la passphrase

## Démarrage rapide

### Compilation

```bash
# Prérequis : Rust 1.85+, protoc (pour tonic/prost)
cargo build --release --workspace

# Avec des features optionnelles
cargo build --release -p enigma-cli --features azure-keyvault,gcp-secretmanager,aws-secretsmanager
cargo build --release -p enigma-proxy --features tls,metrics,azure-keyvault,gcp-secretmanager,aws-secretsmanager

# Emplacement des binaires
ls target/release/enigma        # CLI
ls target/release/enigma-proxy  # Passerelle S3
```

### Utilisation CLI

```bash
# Initialisation (crée la config + fichier de clés chiffré)
enigma --config-dir ~/.enigma --passphrase "my-secret" init

# Sauvegarder un répertoire
enigma --passphrase "my-secret" backup /path/to/data

# Lister les sauvegardes
enigma list

# Vérifier l'intégrité
enigma --passphrase "my-secret" verify <backup-id>

# Restaurer (complet)
enigma --passphrase "my-secret" restore <backup-id> /path/to/restore

# Restauration sélective
enigma --passphrase "my-secret" restore <backup-id> /dest --path docs/     # filtre par préfixe
enigma --passphrase "my-secret" restore <backup-id> /dest --glob "*.rs"    # filtre glob
enigma --passphrase "my-secret" restore <backup-id> /dest --list           # lister les fichiers uniquement

# Ramasse-miettes
enigma gc --dry-run    # lister les blocs orphelins
enigma gc              # supprimer les blocs orphelins

# Chiffrer un identifiant pour la config
enigma --passphrase "my-secret" encrypt-cred "my-aws-secret-key"

# Afficher le statut / la config
enigma status
enigma config
```

### Passerelle S3 (nœud unique)

```bash
# Démarrer le proxy
enigma-proxy --config dev/config-single.toml --passphrase "my-secret"

# Utiliser n'importe quel client S3
aws --endpoint-url http://localhost:8333 s3 mb s3://my-bucket
aws --endpoint-url http://localhost:8333 s3 cp file.txt s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 ls s3://my-bucket/
aws --endpoint-url http://localhost:8333 s3 cp s3://my-bucket/file.txt restored.txt
```

## Configuration

### Référence complète (`enigma.toml`)

```toml
[enigma]
db_path = "/home/user/.enigma/enigma.db"
key_provider = "local"                    # "local" | "azure-keyvault" | "gcp-secretmanager" | "aws-secretsmanager"
keyfile_path = "/home/user/.enigma/keys.enc"
distribution = "RoundRobin"              # "RoundRobin" | "Weighted"
# vault_url = "https://my-vault.vault.azure.net/"  # pour azure-keyvault
# gcp_project_id = "my-project"                     # pour gcp-secretmanager
# aws_region = "us-east-1"                          # pour aws-secretsmanager
# secret_prefix = "enigma-key"                      # préfixe pour les noms de secrets vault

# Découpage — choisir un :
[enigma.chunk_strategy.Cdc]
target_size = 4194304                    # 4 Mo (défaut)

# [enigma.chunk_strategy.Fixed]
# size = 1048576                         # 1 Mo

# Compression (optionnelle, désactivée par défaut)
[enigma.compression]
enabled = false                          # mettre à true pour activer zstd
level = 3                                # niveau zstd 1-22 (défaut : 3)

# Proxy S3 (enigma-proxy uniquement)
[s3_proxy]
listen_addr = "0.0.0.0:8333"
access_key = "enigma-admin"
secret_key = "enigma-secret"
default_region = "us-east-1"
# tls_cert = "/path/to/cert.pem"         # active HTTPS (feature : tls)
# tls_key = "/path/to/key.pem"
# metrics_addr = "0.0.0.0:9090"          # endpoint Prometheus (feature : metrics)

# Fournisseurs de stockage — ajoutez-en autant que nécessaire
[[providers]]
name = "aws-main"
type = "S3"
bucket = "my-enigma-bucket"
region = "eu-west-1"
weight = 2

[[providers]]
name = "rustfs-local"
type = "S3Compatible"                    # Accepte aussi : "minio", "rustfs", "garage"
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
bucket = "enigma-container"              # Nom du conteneur
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
bucket = "/data/enigma-local"            # Chemin du répertoire local
weight = 1

# Raft (optionnel, pour la HA multi-nœuds)
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

### Types de fournisseurs

| Type | Valeur(s) | Notes |
|------|-----------|-------|
| Système de fichiers local | `Local` | `bucket` = chemin du répertoire |
| AWS S3 | `S3` | Utilise la chaîne d'identifiants par défaut AWS SDK |
| S3-compatible | `S3Compatible`, `minio`, `rustfs`, `garage` | Nécessite `endpoint_url`, `path_style = true` |
| Azure Blob Storage | `Azure` | `bucket` = nom du conteneur |
| Google Cloud Storage | `Gcs` | Utilise les Application Default Credentials |

### Variables d'environnement

| Variable | Description |
|----------|-------------|
| `ENIGMA_PASSPHRASE` | Passphrase pour le chiffrement des clés (évite le prompt interactif) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | Identifiants AWS (pour le fournisseur S3) |
| `AZURE_STORAGE_ACCOUNT` / `AZURE_STORAGE_KEY` | Identifiants Azure |
| `GOOGLE_APPLICATION_CREDENTIALS` | Chemin vers le JSON du compte de service GCP |
| `AWS_REGION` | Région AWS pour le fournisseur de clés Secrets Manager |
| `RUST_LOG` | Filtre de niveau de log (ex : `enigma=info,tower=warn`) |

## Compatibilité API S3

| Opération | Supportée |
|-----------|-----------|
| CreateBucket | Oui |
| DeleteBucket | Oui (doit être vide) |
| HeadBucket | Oui |
| ListBuckets | Oui |
| PutObject | Oui |
| GetObject | Oui |
| HeadObject | Oui |
| DeleteObject | Oui |
| ListObjectsV2 | Oui (prefix, delimiter, max-keys, continuation-token) |
| CreateMultipartUpload | Oui |
| UploadPart | Oui |
| CompleteMultipartUpload | Oui |
| AbortMultipartUpload | Oui |

## Tests

### Tests unitaires et d'intégration (49+ tests)

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

### Tests Vault (nécessitent de vrais identifiants)

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

### Couverture des tests

| Module | Ce qui est testé |
|--------|-----------------|
| `chunk::cdc` | Fichier vide, petit fichier (bloc unique), gros fichier (multi-blocs), hashes déterministes |
| `chunk::fixed` | Fichier vide, multiple exact, gestion du reste |
| `compression` | Aller-retour compress/décompress, données vides |
| `config` | Sérialisation TOML aller-retour, erreur fichier manquant |
| `config::credentials` | Aller-retour chiffrement/déchiffrement, passthrough texte clair |
| `crypto` | Aller-retour chiffrement/déchiffrement (brut + bloc), rejet mauvaise clé, rejet mauvais AAD, nonces uniques |
| `dedup` | Hachage déterministe, données différentes = hashes différents, détection des doublons |
| `distributor` | Cycle round-robin, distribution pondérée, recherche de fournisseur |
| `manifest::schema` | Création de tables, idempotence des migrations |
| `manifest::queries` | Flux de sauvegarde complet, ordre de listing, comptage de références des blocs, journaux |
| `types` | Aller-retour hex ChunkHash, format clé de stockage, zéroisation KeyMaterial, parsing ProviderType |
| `keys::local` | Créer/ouvrir fichier de clés, mauvaise passphrase, tailles ML-KEM, indépendance des clés hybrides, rotation |
| `keys::vault` | Azure KV, GCP SM, AWS SM — créer, obtenir, tourner, lister (intégration) |
| `storage::local` | Test de connexion, aller-retour upload/download, aller-retour manifeste |

### Performance (Apple M3 Pro, build release)

```bash
cargo test --release -p enigma-core --test bench_pipeline -- --nocapture
cargo test --release -p enigma-keys --test bench_keys -- --nocapture
```

#### Débit du pipeline

| Étape | 1 Mo | 4 Mo | 16 Mo |
|-------|------|------|-------|
| Hachage SHA-256 | 340 Mo/s | 318 Mo/s | 339 Mo/s |
| Chiffrement AES-256-GCM | 135 Mo/s | 135 Mo/s | 137 Mo/s |
| Déchiffrement AES-256-GCM | 137 Mo/s | 135 Mo/s | 137 Mo/s |
| Compression zstd (aléatoire) | 4224 Mo/s | 2484 Mo/s | 1830 Mo/s |
| Compression zstd (texte) | 6762 Mo/s | 6242 Mo/s | — |

#### Découpage

| Moteur | Fichier 4 Mo | Fichier 16 Mo | Fichier 64 Mo |
|--------|-------------|--------------|--------------|
| CDC (cible 4 Mo) | 271 Mo/s | 227 Mo/s | 266 Mo/s |
| Fixed (4 Mo) | 308 Mo/s | 221 Mo/s | 310 Mo/s |

#### Pipeline complet (Découpe -> Hachage -> Compression -> Chiffrement)

| Entrée | Blocs | Débit |
|--------|-------|-------|
| 4 Mo | 1 | 70 Mo/s |
| 16 Mo | 2-3 | 66 Mo/s |
| 64 Mo | 10-16 | 69 Mo/s |

#### Dérivation de clé (Argon2id + ML-KEM-768 + HKDF)

| Opération | Temps |
|-----------|-------|
| Création (keygen + chiffrement) | 17 ms |
| Ouverture (déchiffrement + dérivation) | 15 ms |

> Le goulot d'étranglement est AES-256-GCM (~135 Mo/s). SHA-256 et zstd sont beaucoup plus rapides.
> Les E/S réseau vers les backends cloud sont généralement le vrai goulot d'étranglement en production.

### Test E2E

```bash
# Nécessite 3 instances RustFS en cours d'exécution (cluster Kind ou docker-compose)
./tests/e2e_rustfs.sh
```

Tests : init -> sauvegarde 5 fichiers -> vérification -> restauration -> diff original vs restauré.

### Pipeline CI

GitHub Actions s'exécute à chaque push/PR vers `main` :
- **Format** — `cargo fmt --check`
- **Clippy** — `cargo clippy --workspace`
- **Test** — `cargo test --workspace`

## Déploiement

### Docker Compose (cluster 3 nœuds)

```bash
docker compose up -d
# 3 nœuds enigma-proxy (ports 8333-8335) + 3 backends RustFS (ports 19001-19003)

# Test
aws --endpoint-url http://localhost:8333 s3 mb s3://test
aws --endpoint-url http://localhost:8333 s3 cp README.md s3://test/
```

### Kubernetes (StatefulSet)

```bash
kubectl apply -f k8s/rustfs.yaml
kubectl apply -f k8s/enigma-cluster.yaml

# 3 pods enigma (StatefulSet) + 3 déploiements RustFS
# Accès S3 via le service ClusterIP enigma-s3 sur le port 8333
```

### Binaire unique

```bash
# Mode CLI (sauvegarde/restauration)
enigma --config-dir /etc/enigma backup /data

# Mode passerelle (proxy S3)
enigma-proxy --config /etc/enigma/config.toml
```

## Feuille de route

- [x] Intégration Vault pour les secrets (AWS Secrets Manager, Azure Key Vault, GCP Secret Manager)
- [x] Endpoint de métriques Prometheus
- [x] Support TLS pour la passerelle S3
- [x] Identifiants chiffrés dans la config
- [x] Ramasse-miettes pour les blocs orphelins
- [x] Restauration sélective (filtres path/glob)
- [ ] Sauvegardes incrémentales (fichiers modifiés uniquement)
- [ ] Limitation de bande passante
- [ ] Tableau de bord Web UI
- [ ] Récupération Raft basée sur les snapshots
- [ ] Codage par effacement (Reed-Solomon) comme alternative à la réplication

## Licence

Source-Available (voir [LICENSE](LICENSE))
