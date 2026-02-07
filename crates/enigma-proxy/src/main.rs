#[cfg(feature = "metrics")]
mod metrics;

use std::collections::{BTreeMap, BTreeSet, HashMap};
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
use enigma_s3::EnigmaS3State;
use enigma_s3::auth::EnigmaS3Auth;
use enigma_s3::service::EnigmaS3Service;
use enigma_storage::provider::StorageProvider;
use enigma_storage::s3::S3StorageProvider;

#[cfg(feature = "azure")]
use enigma_storage::azure::AzureStorageProvider;
#[cfg(feature = "gcs")]
use enigma_storage::gcs::GcsStorageProvider;

// ── RaftClusterHandle ────────────────────────────────────────────

#[cfg(feature = "web")]
struct RaftClusterHandle {
    raft: Arc<enigma_raft::EnigmaRaft>,
    node_id: u64,
    peers: Arc<Mutex<HashMap<u64, String>>>,
}

#[cfg(feature = "web")]
#[async_trait::async_trait]
impl enigma_web::cluster_handle::ClusterHandle for RaftClusterHandle {
    async fn metrics(&self) -> serde_json::Value {
        let m = self.raft.metrics().borrow().clone();
        let peers: Vec<serde_json::Value> = {
            let p = self.peers.lock().unwrap();
            p.iter()
                .map(|(id, addr)| serde_json::json!({ "id": id, "addr": addr }))
                .collect()
        };

        serde_json::json!({
            "mode": "raft",
            "node_id": self.node_id,
            "state": format!("{:?}", m.state),
            "current_leader": m.current_leader,
            "current_term": m.current_term,
            "last_applied": m.last_applied.map(|l| l.index),
            "last_log_index": m.last_log_index,
            "snapshot_index": m.snapshot.map(|l| l.index),
            "membership": format!("{:?}", m.membership_config),
            "peers": peers,
        })
    }

    async fn add_node(&self, node_id: u64, addr: String) -> anyhow::Result<()> {
        // Add as learner first
        self.raft
            .add_learner(node_id, openraft::BasicNode { addr: addr.clone() }, true)
            .await?;

        // Then promote to voter — get current voter IDs and add the new node
        let m = self.raft.metrics().borrow().clone();
        let mut voter_ids: BTreeSet<u64> = m
            .membership_config
            .membership()
            .voter_ids()
            .collect();
        voter_ids.insert(node_id);
        self.raft.change_membership(voter_ids, false).await?;

        // Update peer map
        self.peers.lock().unwrap().insert(node_id, addr);
        Ok(())
    }

    async fn remove_node(&self, node_id: u64) -> anyhow::Result<()> {
        // Get current voter IDs and remove the node
        let m = self.raft.metrics().borrow().clone();
        let voter_ids: BTreeSet<u64> = m
            .membership_config
            .membership()
            .voter_ids()
            .filter(|id| *id != node_id)
            .collect();
        self.raft.change_membership(voter_ids, false).await?;

        // Update peer map
        self.peers.lock().unwrap().remove(&node_id);
        Ok(())
    }

    async fn trigger_snapshot(&self) -> anyhow::Result<()> {
        self.raft.trigger().snapshot().await?;
        Ok(())
    }
}

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

/// Extended proxy config that includes S3 proxy + Raft + Web sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProxyConfig {
    enigma: enigma_core::config::EnigmaSettings,
    #[serde(default)]
    providers: Vec<ProviderConfig>,
    #[serde(default)]
    s3_proxy: S3ProxyConfig,
    #[serde(default)]
    raft: Option<enigma_raft::config::RaftConfig>,
    #[cfg(feature = "web")]
    #[serde(default)]
    web: Option<enigma_web::WebConfig>,
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
    /// Path to TLS certificate PEM file (enables HTTPS).
    #[serde(default)]
    tls_cert: Option<String>,
    /// Path to TLS private key PEM file.
    #[serde(default)]
    tls_key: Option<String>,
    /// Address for the Prometheus metrics endpoint (e.g. "0.0.0.0:9090").
    #[serde(default)]
    metrics_addr: Option<String>,
}

impl Default for S3ProxyConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            access_key: default_access_key(),
            secret_key: default_secret_key(),
            default_region: default_region(),
            tls_cert: None,
            tls_key: None,
            metrics_addr: None,
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

    // Open manifest DB (shared between S3 state and Raft state machine)
    let db = ManifestDb::open(Path::new(&proxy_config.enigma.db_path))?;
    let shared_db = Arc::new(Mutex::new(db));

    // Get encryption key via factory
    let passphrase = if proxy_config.enigma.key_provider == "local" {
        Some(get_passphrase(&cli.passphrase)?)
    } else {
        None
    };
    let mut key_provider = enigma_keys::factory::create_key_provider(
        &proxy_config.enigma.key_provider,
        passphrase.as_deref().map(|s| s.as_bytes()),
        &proxy_config.enigma.keyfile_path,
        proxy_config.enigma.vault_url.as_deref(),
        proxy_config.enigma.gcp_project_id.as_deref(),
        proxy_config.enigma.aws_region.as_deref(),
        proxy_config.enigma.secret_prefix.as_deref(),
    )
    .await?;

    // Try to get existing key; if none exists yet (first run), create one
    let managed_key = match key_provider.get_current_key().await {
        Ok(k) => k,
        Err(_) => {
            tracing::info!("No key found in provider — creating initial key");
            key_provider.create_key().await?
        }
    };
    let key_material = KeyMaterial {
        id: managed_key.id.clone(),
        key: managed_key.key,
    };

    // Initialize storage providers
    let mut storage_providers: HashMap<i64, Box<dyn StorageProvider>> = HashMap::new();

    for pc in &proxy_config.providers {
        let existing = shared_db.lock().unwrap().list_providers()?;
        let pid = match existing.iter().find(|p| p.name == pc.name) {
            Some(p) => p.id,
            None => shared_db.lock().unwrap().insert_provider(
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
            #[cfg(feature = "azure")]
            ProviderType::Azure => {
                let account = pc.access_key.as_deref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "Azure provider '{}' requires access_key (storage account name)",
                        pc.name
                    )
                })?;
                let key = pc.secret_key.as_deref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "Azure provider '{}' requires secret_key (storage account key)",
                        pc.name
                    )
                })?;
                Box::new(AzureStorageProvider::new(account, key, &pc.bucket, &pc.name)?)
            }
            #[cfg(feature = "gcs")]
            ProviderType::Gcs => {
                Box::new(GcsStorageProvider::new(&pc.bucket, &pc.name).await?)
            }
            _ => {
                anyhow::bail!("Unsupported provider type: {:?}", pc.provider_type);
            }
        };

        tracing::info!("Testing connection to provider '{}'...", pc.name);
        provider.test_connection().await?;
        tracing::info!("Provider '{}' OK", pc.name);
        storage_providers.insert(pid, provider);
    }

    let provider_infos = shared_db.lock().unwrap().list_providers()?;

    // If no providers are loaded, re-create them from DB entries or create a local fallback
    if storage_providers.is_empty() {
        if provider_infos.is_empty() {
            // First run: no providers in DB — create a local fallback
            let local_path = Path::new(&proxy_config.enigma.db_path)
                .parent()
                .unwrap_or(Path::new("."))
                .join("storage");
            std::fs::create_dir_all(&local_path)?;
            let pid = shared_db.lock().unwrap().insert_provider(
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
        } else {
            // Restart: providers exist in DB but weren't loaded from config — re-open local ones
            for pi in &provider_infos {
                if pi.provider_type == ProviderType::Local {
                    let provider = enigma_storage::local::LocalStorageProvider::new(
                        Path::new(&pi.bucket),
                        &pi.name,
                    )?;
                    storage_providers.insert(pi.id, Box::new(provider));
                    tracing::info!("Re-opened local provider '{}' at {}", pi.name, pi.bucket);
                } else {
                    tracing::warn!(
                        "Provider '{}' ({:?}) exists in DB but is not configured — skipping",
                        pi.name,
                        pi.provider_type
                    );
                }
            }
        }
    }

    let provider_infos = shared_db.lock().unwrap().list_providers()?;

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
        db: shared_db.clone(),
        providers: storage_providers,
        distributor,
        key_material,
        config: enigma_config,
    });

    // Build S3 service
    let s3_service = EnigmaS3Service::new(state.clone());

    let mut s3_builder = S3ServiceBuilder::new(s3_service);

    // Setup auth
    let auth = EnigmaS3Auth::new(
        proxy_config.s3_proxy.access_key.clone(),
        proxy_config.s3_proxy.secret_key.clone(),
    );
    s3_builder.set_auth(auth);

    let s3_service = s3_builder.build();

    // Optionally start Prometheus metrics server
    #[cfg(feature = "metrics")]
    if let Some(ref metrics_addr) = proxy_config.s3_proxy.metrics_addr {
        let addr: SocketAddr = metrics_addr.parse()?;
        tracing::info!("Starting metrics server on {addr}");
        tokio::spawn(metrics::serve_metrics(addr));
    }

    // Determine if we're in multi-node Raft mode
    let is_multi_node = proxy_config
        .raft
        .as_ref()
        .is_some_and(|rc| !rc.is_single_node());

    if is_multi_node {
        let raft_config = proxy_config.raft.as_ref().unwrap();
        let node_id = raft_config.node_id;

        // ── Recovery mode ────────────────────────────────────
        if raft_config.force_new_cluster {
            tracing::warn!("RECOVERY MODE: wiping Raft log to bootstrap as single node");
            let log_path = format!("{}/raft-log.db", raft_config.data_dir);
            for ext in ["", "-wal", "-shm"] {
                let p = format!("{log_path}{ext}");
                if Path::new(&p).exists() {
                    std::fs::remove_file(&p)?;
                    tracing::info!("Removed {p}");
                }
            }
        }

        // Use only self when in recovery mode
        let effective_peers: Vec<enigma_raft::config::PeerConfig> =
            if raft_config.force_new_cluster {
                vec![enigma_raft::config::PeerConfig {
                    id: node_id,
                    addr: raft_config.grpc_addr.clone(),
                }]
            } else {
                raft_config.peers.clone()
            };

        tracing::info!(
            "Raft mode: node_id={}, peers={}{}",
            node_id,
            effective_peers.len(),
            if raft_config.force_new_cluster {
                " (RECOVERY)"
            } else {
                ""
            }
        );

        // Build peer address map
        let peer_map: HashMap<u64, String> = effective_peers
            .iter()
            .map(|p| (p.id, p.addr.clone()))
            .collect();

        // Create Raft components
        let log_store_path = format!("{}/raft-log.db", raft_config.data_dir);
        tracing::info!("Opening Raft log store at {log_store_path}");
        let log_store = enigma_raft::log_store::SqliteLogStore::new(&log_store_path)?;
        tracing::info!("Log store opened, creating state machine");
        let state_machine = enigma_raft::state_machine::EnigmaStateMachine::new(
            shared_db.clone(),
            proxy_config.enigma.db_path.clone(),
        );
        let network = enigma_raft::network::EnigmaNetworkFactory::new(peer_map.clone());
        let shared_peers = network.peers.clone();
        tracing::info!("Creating Raft engine...");

        // Build openraft Config
        let raft_openraft_config = openraft::Config {
            election_timeout_min: raft_config.election_timeout_ms,
            election_timeout_max: raft_config.election_timeout_ms * 2,
            heartbeat_interval: raft_config.heartbeat_interval_ms,
            snapshot_policy: openraft::SnapshotPolicy::LogsSinceLast(
                raft_config.snapshot_threshold,
            ),
            ..Default::default()
        };
        let raft_openraft_config = Arc::new(raft_openraft_config.validate()?);

        // Create Raft engine
        let raft = openraft::Raft::<enigma_raft::TypeConfig>::new(
            node_id,
            raft_openraft_config,
            network,
            log_store,
            state_machine,
        )
        .await?;
        tracing::info!("Raft engine created successfully");
        let raft = Arc::new(raft);

        // Start gRPC server for inter-node communication
        let grpc_addr: SocketAddr = raft_config.grpc_addr.parse()?;
        let grpc_server = enigma_raft::grpc_server::EnigmaRaftGrpcServer::new(raft.clone());
        let grpc_svc =
            enigma_raft::proto::raft_service_server::RaftServiceServer::new(grpc_server);

        tracing::info!("Starting Raft gRPC server on {grpc_addr}");
        tokio::spawn(async move {
            if let Err(e) = tonic::transport::Server::builder()
                .add_service(grpc_svc)
                .serve(grpc_addr)
                .await
            {
                tracing::error!("Raft gRPC server error: {e}");
            }
        });

        // Bootstrap cluster from the node with the smallest ID
        let min_peer_id = effective_peers.iter().map(|p| p.id).min().unwrap_or(1);
        if node_id == min_peer_id {
            let mut members = BTreeMap::new();
            for peer in &effective_peers {
                members.insert(
                    peer.id,
                    openraft::BasicNode {
                        addr: peer.addr.clone(),
                    },
                );
            }
            match raft.initialize(members).await {
                Ok(_) => tracing::info!("Raft cluster initialized from node {node_id}"),
                Err(e) => {
                    // Ignore if already initialized
                    tracing::debug!("Raft initialize returned (already done?): {e}");
                }
            }
        }

        // Build cluster handle for web UI
        #[cfg(feature = "web")]
        let cluster_handle: Option<Arc<dyn enigma_web::cluster_handle::ClusterHandle>> = Some(
            Arc::new(RaftClusterHandle {
                raft: raft.clone(),
                node_id,
                peers: shared_peers,
            }),
        );

        // Spawn leadership watch — start/stop web UI based on leadership
        #[cfg(feature = "web")]
        {
            let web_config = proxy_config.web.clone();
            let db_path = proxy_config.enigma.db_path.clone();
            let enigma_settings = proxy_config.enigma.clone();
            let s3_state_for_web = state.clone();
            let cluster_handle_for_web = cluster_handle.clone();
            let mut metrics_rx = raft.metrics();

            tokio::spawn(async move {
                let mut web_handle: Option<(
                    tokio::task::JoinHandle<()>,
                    tokio::sync::oneshot::Sender<()>,
                )> = None;

                loop {
                    let is_leader = {
                        let m = metrics_rx.borrow();
                        m.state == openraft::ServerState::Leader
                    };

                    if let Some(ref wc) = web_config {
                        if is_leader && web_handle.is_none() {
                            tracing::info!("Leader — starting web UI");
                            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
                            let wc = wc.clone();
                            let db_path = db_path.clone();
                            let enigma_settings = enigma_settings.clone();
                            let s3_state = Some(s3_state_for_web.clone());
                            let cluster = cluster_handle_for_web.clone();
                            let handle = tokio::spawn(async move {
                                if let Err(e) = enigma_web::start_web_server(
                                    wc,
                                    &db_path,
                                    enigma_settings,
                                    s3_state,
                                    Some(shutdown_rx),
                                    cluster,
                                )
                                .await
                                {
                                    tracing::error!("Web UI server error: {e}");
                                }
                            });
                            web_handle = Some((handle, shutdown_tx));
                        } else if !is_leader && web_handle.is_some() {
                            tracing::info!("Lost leadership — stopping web UI");
                            let (handle, tx) = web_handle.take().unwrap();
                            let _ = tx.send(());
                            let _ = handle.await;
                        }
                    }

                    if metrics_rx.changed().await.is_err() {
                        tracing::warn!("Raft metrics channel closed — exiting leadership watch");
                        break;
                    }
                }
            });
        }
    } else {
        // Single-node mode: no Raft, start web UI directly
        if proxy_config.raft.is_some() {
            tracing::info!("Single-node mode (Raft disabled)");
        } else {
            tracing::info!("No Raft config — running as single node");
        }

        #[cfg(feature = "web")]
        if let Some(web_config) = proxy_config.web.clone() {
            let db_path = proxy_config.enigma.db_path.clone();
            let enigma_settings = proxy_config.enigma.clone();
            let s3_state_for_web = Some(state.clone());
            tokio::spawn(async move {
                if let Err(e) = enigma_web::start_web_server(
                    web_config,
                    &db_path,
                    enigma_settings,
                    s3_state_for_web,
                    None,
                    None,
                )
                .await
                {
                    tracing::error!("Web UI server error: {e}");
                }
            });
        }
    }

    // Start HTTP/HTTPS server (runs in all modes: single-node and multi-node)
    let addr: SocketAddr = proxy_config.s3_proxy.listen_addr.parse()?;
    tracing::info!("Starting Enigma S3 proxy on {addr}");
    tracing::info!("  Access key: {}", proxy_config.s3_proxy.access_key);
    tracing::info!("  Region: {}", proxy_config.s3_proxy.default_region);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Use hyper to serve the s3s service
    let shared_service = s3_service.into_shared();

    // Optionally load TLS config
    #[cfg(feature = "tls")]
    let tls_acceptor = match (
        &proxy_config.s3_proxy.tls_cert,
        &proxy_config.s3_proxy.tls_key,
    ) {
        (Some(cert_path), Some(key_path)) => {
            let acceptor = load_tls_config(cert_path, key_path)?;
            tracing::info!("TLS enabled");
            Some(acceptor)
        }
        _ => None,
    };

    loop {
        let (stream, _remote_addr) = listener.accept().await?;
        let service = shared_service.clone();

        #[cfg(feature = "tls")]
        let tls_acceptor = tls_acceptor.clone();

        tokio::spawn(async move {
            let builder =
                hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());

            #[cfg(feature = "tls")]
            if let Some(ref acceptor) = tls_acceptor {
                match acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        let io = hyper_util::rt::TokioIo::new(tls_stream);
                        if let Err(e) = builder.serve_connection(io, service).await {
                            tracing::error!("TLS connection error: {e}");
                        }
                        return;
                    }
                    Err(e) => {
                        tracing::error!("TLS handshake error: {e}");
                        return;
                    }
                }
            }

            let io = hyper_util::rt::TokioIo::new(stream);
            if let Err(e) = builder.serve_connection(io, service).await {
                tracing::error!("Connection error: {e}");
            }
        });
    }
}

#[cfg(feature = "tls")]
fn load_tls_config(cert_path: &str, key_path: &str) -> anyhow::Result<tokio_rustls::TlsAcceptor> {
    use rustls::ServerConfig;
    use std::io::BufReader;
    use tokio_rustls::TlsAcceptor;

    let cert_file = std::fs::File::open(cert_path)
        .map_err(|e| anyhow::anyhow!("Cannot open TLS cert {cert_path}: {e}"))?;
    let key_file = std::fs::File::open(key_path)
        .map_err(|e| anyhow::anyhow!("Cannot open TLS key {key_path}: {e}"))?;

    let certs: Vec<_> = rustls_pemfile::certs(&mut BufReader::new(cert_file))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let key = rustls_pemfile::private_key(&mut BufReader::new(key_file))?
        .ok_or_else(|| anyhow::anyhow!("No private key found in {key_path}"))?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    Ok(TlsAcceptor::from(std::sync::Arc::new(config)))
}
