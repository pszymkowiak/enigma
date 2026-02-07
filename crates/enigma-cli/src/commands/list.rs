use anyhow::Result;
use std::path::Path;

use enigma_core::config::EnigmaConfig;
use enigma_core::manifest::ManifestDb;

pub fn run(base_dir: &Path) -> Result<()> {
    let config_path = EnigmaConfig::default_path(base_dir);
    let config = EnigmaConfig::load(&config_path)?;
    let db = ManifestDb::open(Path::new(&config.enigma.db_path))?;

    let backups = db.list_backups()?;

    if backups.is_empty() {
        println!("No backups found.");
        return Ok(());
    }

    println!(
        "{:<38} {:<12} {:<8} {:>12} {:>8} {}",
        "ID", "STATUS", "FILES", "SIZE", "CHUNKS", "CREATED"
    );
    println!("{}", "-".repeat(100));

    for b in &backups {
        println!(
            "{:<38} {:<12} {:<8} {:>12} {:>8} {}",
            b.id,
            b.status,
            b.total_files,
            format_bytes(b.total_bytes),
            b.total_chunks,
            b.created_at,
        );
    }

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
