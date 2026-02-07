use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};

use enigma_core::chunk::{CdcChunkEngine, ChunkEngine, FixedSizeChunkEngine};
use enigma_core::config::EnigmaConfig;
use enigma_core::crypto::encrypt_chunk;
use enigma_core::distributor::Distributor;
use enigma_core::manifest::ManifestDb;
use enigma_core::types::{ChunkStrategy, DistributionStrategy, KeyMaterial, ProviderType};
use enigma_keys::local::LocalKeyProvider;
use enigma_keys::provider::KeyProvider;
use enigma_storage::local::LocalStorageProvider;

use super::providers::init_providers;

pub async fn run(source: &Path, base_dir: &Path, cli_passphrase: &Option<String>) -> Result<()> {
    let source = source.canonicalize()?;
    println!("Backing up: {}", source.display());

    // Load config
    let config_path = EnigmaConfig::default_path(base_dir);
    let config = EnigmaConfig::load(&config_path)?;

    // Open database
    let db = ManifestDb::open(Path::new(&config.enigma.db_path))?;

    // Get encryption key
    let passphrase = crate::get_passphrase(cli_passphrase)?;
    let keyfile_path = Path::new(&config.enigma.keyfile_path);
    let key_provider = LocalKeyProvider::open(keyfile_path, passphrase.as_bytes())?;
    let managed_key = key_provider.get_current_key().await?;
    let key_material = KeyMaterial {
        id: managed_key.id.clone(),
        key: managed_key.key,
    };

    // Initialize storage providers
    let storage_providers = if config.providers.is_empty() {
        // If no providers configured, use a local fallback
        let local_storage_path = base_dir.join("storage");
        let provider = LocalStorageProvider::new(&local_storage_path, "local-default")?;
        let pid = match db.list_providers()?.first() {
            Some(p) => p.id,
            None => db.insert_provider(
                "local-default",
                ProviderType::Local,
                local_storage_path.to_str().unwrap_or(""),
                None,
                1,
            )?,
        };
        let mut map = std::collections::HashMap::new();
        map.insert(
            pid,
            Box::new(provider) as Box<dyn enigma_storage::provider::StorageProvider>,
        );
        map
    } else {
        init_providers(&config.providers, &db).await?
    };

    let provider_infos = db.list_providers()?;

    // Setup distributor
    let distributor = match config.enigma.distribution {
        DistributionStrategy::RoundRobin => Distributor::round_robin(provider_infos),
        DistributionStrategy::Weighted => Distributor::weighted(provider_infos),
    };

    // Setup chunking engine
    let chunk_engine: Box<dyn ChunkEngine> = match config.enigma.chunk_strategy {
        ChunkStrategy::Cdc { target_size } => Box::new(CdcChunkEngine::new(target_size)),
        ChunkStrategy::Fixed { size } => Box::new(FixedSizeChunkEngine::new(size)),
    };

    // Create backup record
    let backup_id = uuid::Uuid::now_v7().to_string();
    db.create_backup(&backup_id, source.to_str().unwrap_or(""))?;
    db.log(Some(&backup_id), "INFO", "Backup started")?;

    // Walk source directory
    let files = walk_files(&source)?;
    println!("Found {} files", files.len());

    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=>-"),
    );

    let mut total_bytes = 0u64;
    let mut total_chunks = 0u64;
    let mut dedup_chunks = 0u64;

    for file_path in &files {
        let relative = file_path.strip_prefix(&source).unwrap_or(file_path);
        pb.set_message(format!("{}", relative.display()));

        let metadata = std::fs::metadata(file_path)?;
        let file_size = metadata.len();
        total_bytes += file_size;

        // Compute file hash
        let file_hash = {
            use sha2::{Digest, Sha256};
            let data = std::fs::read(file_path)?;
            let mut hasher = Sha256::new();
            hasher.update(&data);
            format!("{:x}", hasher.finalize())
        };

        // Chunk the file
        let chunks = chunk_engine.chunk_file(file_path)?;
        let chunk_count = chunks.len() as u32;

        // Insert file record
        let mtime = metadata.modified().ok().and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs().to_string())
        });
        let file_id = db.insert_backup_file(
            &backup_id,
            relative.to_str().unwrap_or(""),
            file_size,
            mtime.as_deref(),
            &file_hash,
            chunk_count,
        )?;

        // Process each chunk
        let compression = &config.enigma.compression;
        for (idx, chunk) in chunks.iter().enumerate() {
            let hash_hex = chunk.hash.to_hex();
            total_chunks += 1;

            // Pick provider
            let target_provider = distributor.next_provider();
            let storage_key = chunk.hash.storage_key();

            // Compress (optional, before encryption)
            let (data_to_encrypt, size_compressed) = if compression.enabled {
                let compressed =
                    enigma_core::compression::compress_chunk(&chunk.data, compression.level)?;
                let sz = compressed.len() as u64;
                (compressed, Some(sz))
            } else {
                (chunk.data.clone(), None)
            };

            // Encrypt
            let encrypted = encrypt_chunk(&data_to_encrypt, &chunk.hash, &key_material)?;

            // Dedup + upload
            let is_new = db.insert_or_dedup_chunk(
                &hash_hex,
                &encrypted.nonce,
                &key_material.id,
                target_provider.id,
                &storage_key,
                chunk.length as u64,
                encrypted.ciphertext.len() as u64,
                size_compressed,
            )?;

            if is_new {
                // Upload to storage
                if let Some(provider) = storage_providers.get(&target_provider.id) {
                    provider
                        .upload_chunk(&storage_key, &encrypted.ciphertext)
                        .await?;
                }
            } else {
                dedup_chunks += 1;
            }

            // Record file-chunk mapping
            db.insert_file_chunk(file_id, &hash_hex, idx as u32, chunk.offset)?;
        }

        pb.inc(1);
    }

    pb.finish_with_message("done");

    // Complete backup
    db.complete_backup(
        &backup_id,
        files.len() as u64,
        total_bytes,
        total_chunks,
        dedup_chunks,
    )?;
    db.log(Some(&backup_id), "INFO", "Backup completed")?;

    println!("\nBackup completed:");
    println!("  ID:             {backup_id}");
    println!("  Files:          {}", files.len());
    println!("  Total size:     {} bytes", total_bytes);
    println!("  Total chunks:   {total_chunks}");
    println!("  Dedup'd chunks: {dedup_chunks}");

    Ok(())
}

fn walk_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk_recursive(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        // Single file backup
        files.push(dir.to_path_buf());
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_recursive(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}
