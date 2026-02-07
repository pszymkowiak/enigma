use anyhow::Result;
use std::path::Path;

use enigma_core::config::EnigmaConfig;
use enigma_core::manifest::ManifestDb;

pub fn run(base_dir: &Path) -> Result<()> {
    let config_path = EnigmaConfig::default_path(base_dir);
    let config = EnigmaConfig::load(&config_path)?;
    let db = ManifestDb::open(Path::new(&config.enigma.db_path))?;

    match db.latest_backup()? {
        Some(backup) => {
            println!("Latest backup:");
            println!("  ID:             {}", backup.id);
            println!("  Source:         {}", backup.source_path);
            println!("  Status:         {}", backup.status);
            println!("  Files:          {}", backup.total_files);
            println!("  Total size:     {} bytes", backup.total_bytes);
            println!("  Total chunks:   {}", backup.total_chunks);
            println!("  Dedup'd chunks: {}", backup.dedup_chunks);
            println!("  Created:        {}", backup.created_at);
            if let Some(ref completed) = backup.completed_at {
                println!("  Completed:      {completed}");
            }

            // Show providers
            let providers = db.list_providers()?;
            if !providers.is_empty() {
                println!("\n  Providers:");
                for p in &providers {
                    println!(
                        "    - {} ({}) bucket={} weight={}",
                        p.name, p.provider_type, p.bucket, p.weight
                    );
                }
            }
        }
        None => {
            println!("No backups found. Run `enigma backup <path>` to create one.");
        }
    }

    Ok(())
}
