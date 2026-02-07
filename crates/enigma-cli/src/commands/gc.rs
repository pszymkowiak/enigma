use anyhow::Result;
use std::path::Path;

use enigma_core::config::EnigmaConfig;
use enigma_core::manifest::ManifestDb;

use super::providers::init_providers;

pub async fn run(base_dir: &Path, dry_run: bool) -> Result<()> {
    let config_path = EnigmaConfig::default_path(base_dir);
    let config = EnigmaConfig::load(&config_path)?;
    let db = ManifestDb::open(Path::new(&config.enigma.db_path))?;

    let (total, orphan_count) = db.chunk_stats()?;
    println!("Chunk stats: {total} total, {orphan_count} orphans");

    let orphans = db.find_orphan_chunks()?;
    let orphan_replicas = db.find_orphan_chunk_replicas()?;

    if orphans.is_empty() && orphan_replicas.is_empty() {
        println!("No orphaned chunks found.");
        return Ok(());
    }

    println!(
        "Found {} orphaned chunks, {} orphan replicas",
        orphans.len(),
        orphan_replicas.len()
    );

    // Collect all storage locations to delete (primary + replicas)
    let mut all_deletions: Vec<(String, i64, String)> = Vec::new();
    for (hash, provider_id, storage_key) in &orphans {
        all_deletions.push((hash.clone(), *provider_id, storage_key.clone()));
        // Also gather replicas for this orphan chunk
        if let Ok(replicas) = db.get_chunk_replicas(hash) {
            for (pid, skey) in replicas {
                if !all_deletions.iter().any(|(_, p, s)| *p == pid && *s == skey) {
                    all_deletions.push((hash.clone(), pid, skey));
                }
            }
        }
    }
    // Add standalone orphan replicas
    for (hash, pid, skey) in &orphan_replicas {
        if !all_deletions.iter().any(|(_, p, s)| *p == *pid && *s == *skey) {
            all_deletions.push((hash.clone(), *pid, skey.clone()));
        }
    }

    if dry_run {
        println!("\nDry run — would delete {} storage entries:", all_deletions.len());
        for (hash, provider_id, storage_key) in &all_deletions {
            println!("  {hash}  provider={provider_id}  key={storage_key}");
        }
        return Ok(());
    }

    // Initialize storage providers for deletion
    let storage_providers = init_providers(&config.providers, &db).await?;

    let mut deleted = 0u64;
    let mut errors = 0u64;

    // Delete storage objects
    for (_hash, provider_id, storage_key) in &all_deletions {
        if let Some(provider) = storage_providers.get(provider_id) {
            match provider.delete_chunk(storage_key).await {
                Ok(_) => {
                    deleted += 1;
                }
                Err(e) => {
                    eprintln!(
                        "WARN: Failed to delete {storage_key} from provider {provider_id}: {e}"
                    );
                    errors += 1;
                }
            }
        }
    }

    // Delete chunk records from DB (cascades to chunk_replicas)
    for (hash, _provider_id, _storage_key) in &orphans {
        db.delete_chunk_record(hash)?;
    }

    println!("\nGC completed: {deleted} storage entries deleted, {errors} errors");
    Ok(())
}
