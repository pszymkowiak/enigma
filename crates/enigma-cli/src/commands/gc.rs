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
    if orphans.is_empty() {
        println!("No orphaned chunks found.");
        return Ok(());
    }

    println!("Found {} orphaned chunks", orphans.len());

    if dry_run {
        println!("\nDry run — would delete:");
        for (hash, provider_id, storage_key) in &orphans {
            println!("  {hash}  provider={provider_id}  key={storage_key}");
        }
        return Ok(());
    }

    // Initialize storage providers for deletion
    let storage_providers = init_providers(&config.providers, &db).await?;

    let mut deleted = 0u64;
    let mut errors = 0u64;

    for (hash, provider_id, storage_key) in &orphans {
        // Delete from storage
        if let Some(provider) = storage_providers.get(provider_id) {
            match provider.delete_chunk(storage_key).await {
                Ok(_) => {}
                Err(e) => {
                    eprintln!(
                        "WARN: Failed to delete {storage_key} from provider {provider_id}: {e}"
                    );
                    errors += 1;
                    continue;
                }
            }
        }

        // Delete from DB
        db.delete_chunk_record(hash)?;
        deleted += 1;
    }

    println!("\nGC completed: {deleted} chunks deleted, {errors} errors");
    Ok(())
}
