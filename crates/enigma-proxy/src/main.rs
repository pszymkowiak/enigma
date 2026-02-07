use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use clap::Parser;
use s3s::service::S3ServiceBuilder;
use serde::{Deserialize, Serialize};

use enigma_core::config::{EnigmaConfig, ProviderConfig};
use enigma_core::distributor::Distributor;
use enigma_core::manifest::ManifestDb;
use enigma_core::types::{DistributionStrategy, KeyMaterial, ProviderType};
use enigma_keys::local::LocalKeyProvider;
use enigma_keys::provider::KeyProvider;
use enigma_s3::EnigmaS3State;
use enigma_s3::auth::EnigmaS3Auth;
use enigma_s3::service::EnigmaS3Service;
use enigma_storage::provider::StorageProvider;
use enigma_storage::s3::S3StorageProvider;

#[derive(Parser)]
#[command(name = "enigma-proxy")]
#[command(about = "Enigma S3-compatible proxy — encrypted, deduplicated, multi-cloud storage")]
struct Cli {
    /// Path to the configuration file (TOML)
    #[arg(long, short)]
    config: PathBuf,

    /// Passphrase for key encryption (or set ENIGMA_PASSPHRASE env var)
    #[arg(long, env = "ENIGMA_PASSPHRASE")]
    passphrase: Option<String>,
}

/// Extended proxy config that includes S3 proxy + Raft sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProxyConfig {
    enigma: enigma_core::config::EnigmaSettings,
    #[serde(default)]
    providers: Vec<ProviderConfig>,
    #[serde(default)]
    s3_proxy: S3ProxyConfig,
    #[serde(default)]
    raft: Option<enigma_raft::config::RaftConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct S3ProxyConfig {
    #[serde(default = "default_listen_addr")]
    listen_addr: String,
    #[serde(default = "default_access_key")]
    access_key: String,
    #[serde(default = "default_secret_key")]
    secret_key: String,
    #[serde(default = "default_region")]
    default_region: String,
}

impl Default for S3ProxyConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            access_key: default_access_key(),
            secret_key: default_secret_key(),
            default_region: default_region(),
        }
    }
}

fn default_listen_addr() -> String {
    "0.0.0.0:8333".to_string()
}
fn default_access_key() -> String {
    "enigma-admin".to_string()
}
fn default_secret_key() -> String {
    "enigma-secret".to_string()
}
fn default_region() -> String {
    "us-east-1".to_string()
}

fn get_passphrase(cli_passphrase: &Option<String>) -> anyhow::Result<String> {
    if let Some(p) = cli_passphrase {
        return Ok(p.clone());
    }
    use std::io::{self, Write};
    print!("Enter passphrase: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("enigma=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    // Load config
    let config_content = std::fs::read_to_string(&cli.config)?;
    let proxy_config: ProxyConfig = toml::from_str(&config_content)?;

    tracing::info!("Loading configuration from {}", cli.config.display());

    // Open manifest DB
    let db = ManifestDb::open(Path::new(&proxy_config.enigma.db_path))?;

    // Get encryption key
    let passphrase = get_passphrase(&cli.passphrase)?;
    let keyfile_path = Path::new(&proxy_config.enigma.keyfile_path);

    // Create keyfile if it doesn't exist
    let key_provider = if keyfile_path.exists() {
        LocalKeyProvider::open(keyfile_path, passphrase.as_bytes())?
    } else {
        tracing::info!("Creating new keyfile at {}", keyfile_path.display());
        LocalKeyProvider::create(keyfile_path, passphrase.as_bytes())?
    };

    let managed_key = key_provider.get_current_key().await?;
    let key_material = KeyMaterial {
        id: managed_key.id.clone(),
        key: managed_key.key,
    };

    // Initialize storage providers
    let mut storage_providers: HashMap<i64, Box<dyn StorageProvider>> = HashMap::new();

    for pc in &proxy_config.providers {
        let existing = db.list_providers()?;
        let pid = match existing.iter().find(|p| p.name == pc.name) {
            Some(p) => p.id,
            None => db.insert_provider(
                &pc.name,
                pc.provider_type,
                &pc.bucket,
                pc.region.as_deref(),
                pc.weight,
            )?,
        };

        let provider: Box<dyn StorageProvider> = match pc.provider_type {
            ProviderType::S3Compatible => {
                let endpoint = pc.endpoint_url.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("S3Compatible provider '{}' requires endpoint_url", pc.name)
                })?;
                Box::new(
                    S3StorageProvider::s3_compatible(
                        &pc.bucket,
                        endpoint,
                        pc.region.as_deref(),
                        &pc.name,
                        pc.access_key.as_deref(),
                        pc.secret_key.as_deref(),
                    )
                    .await?,
                )
            }
            ProviderType::S3 => {
                Box::new(S3StorageProvider::new(&pc.bucket, pc.region.as_deref(), &pc.name).await?)
            }
            ProviderType::Local => Box::new(enigma_storage::local::LocalStorageProvider::new(
                Path::new(&pc.bucket),
                &pc.name,
            )?),
            _ => {
                anyhow::bail!("Unsupported provider type: {:?}", pc.provider_type);
            }
        };

        tracing::info!("Testing connection to provider '{}'...", pc.name);
        provider.test_connection().await?;
        tracing::info!("Provider '{}' OK", pc.name);
        storage_providers.insert(pid, provider);
    }

    let provider_infos = db.list_providers()?;

    // If no providers configured, create a local fallback
    if provider_infos.is_empty() {
        let local_path = Path::new(&proxy_config.enigma.db_path)
            .parent()
            .unwrap_or(Path::new("."))
            .join("storage");
        std::fs::create_dir_all(&local_path)?;
        let pid = db.insert_provider(
            "local-default",
            ProviderType::Local,
            local_path.to_str().unwrap_or(""),
            None,
            1,
        )?;
        let provider =
            enigma_storage::local::LocalStorageProvider::new(&local_path, "local-default")?;
        storage_providers.insert(pid, Box::new(provider));
        tracing::info!(
            "No providers configured, using local fallback at {}",
            local_path.display()
        );
    }

    let provider_infos = db.list_providers()?;

    // Setup distributor
    let distributor = match proxy_config.enigma.distribution {
        DistributionStrategy::RoundRobin => Distributor::round_robin(provider_infos),
        DistributionStrategy::Weighted => Distributor::weighted(provider_infos),
    };

    // Build the EnigmaConfig for the state
    let enigma_config = EnigmaConfig {
        enigma: proxy_config.enigma.clone(),
        providers: proxy_config.providers.clone(),
    };

    // Create shared state
    let state = Arc::new(EnigmaS3State {
        db: Mutex::new(db),
        providers: storage_providers,
        distributor,
        key_material,
        config: enigma_config,
    });

    // Build S3 service
    let s3_service = EnigmaS3Service::new(state);

    let mut s3_builder = S3ServiceBuilder::new(s3_service);

    // Setup auth
    let auth = EnigmaS3Auth::new(
        proxy_config.s3_proxy.access_key.clone(),
        proxy_config.s3_proxy.secret_key.clone(),
    );
    s3_builder.set_auth(auth);

    let s3_service = s3_builder.build();

    // Optionally start Raft gRPC server
    if let Some(raft_config) = &proxy_config.raft {
        if !raft_config.is_single_node() {
            tracing::info!(
                "Raft mode: node_id={}, peers={}",
                raft_config.node_id,
                raft_config.peers.len()
            );
            // Raft startup would go here in a full implementation
            // For now, we operate in single-node mode
            tracing::warn!("Multi-node Raft not yet fully wired — running as single node");
        } else {
            tracing::info!("Single-node mode (Raft disabled)");
        }
    } else {
        tracing::info!("No Raft config — running as single node");
    }

    // Start HTTP server
    let addr: SocketAddr = proxy_config.s3_proxy.listen_addr.parse()?;
    tracing::info!("Starting Enigma S3 proxy on {addr}");
    tracing::info!("  Access key: {}", proxy_config.s3_proxy.access_key);
    tracing::info!("  Region: {}", proxy_config.s3_proxy.default_region);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Use hyper to serve the s3s service
    let shared_service = s3_service.into_shared();

    loop {
        let (stream, _remote_addr) = listener.accept().await?;
        let service = shared_service.clone();
        tokio::spawn(async move {
            let io = hyper_util::rt::TokioIo::new(stream);
            let builder =
                hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());
            let conn = builder.serve_connection(io, service);
            if let Err(e) = conn.await {
                tracing::error!("Connection error: {e}");
            }
        });
    }
}
