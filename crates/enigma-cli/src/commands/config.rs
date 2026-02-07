use anyhow::Result;
use std::path::Path;

use enigma_core::config::EnigmaConfig;

pub fn run(base_dir: &Path) -> Result<()> {
    let config_path = EnigmaConfig::default_path(base_dir);
    let config = EnigmaConfig::load(&config_path)?;

    println!("Config: {}", config_path.display());
    println!();
    println!("  DB path:        {}", config.enigma.db_path);
    println!("  Key provider:   {}", config.enigma.key_provider);
    println!("  Keyfile:        {}", config.enigma.keyfile_path);
    println!("  Chunking:       {:?}", config.enigma.chunk_strategy);
    println!("  Distribution:   {:?}", config.enigma.distribution);
    println!();

    if config.providers.is_empty() {
        println!("  No providers configured.");
        println!();
        println!("  Add providers to {}:", config_path.display());
        println!("  [[providers]]");
        println!("  name = \"my-s3\"");
        println!("  type = \"local\"    # or \"s3\", \"azure\", \"gcs\"");
        println!("  bucket = \"/path/to/storage\"");
        println!("  weight = 1");
    } else {
        println!("  Providers ({}):", config.providers.len());
        for p in &config.providers {
            println!(
                "    - {} (type={}, bucket={}, weight={})",
                p.name, p.provider_type, p.bucket, p.weight
            );
        }
    }

    Ok(())
}
