use anyhow::Result;
use std::path::Path;

use enigma_core::config::EnigmaConfig;
use enigma_core::manifest::ManifestDb;
use enigma_keys::local::LocalKeyProvider;

pub fn run(base_dir: &Path, cli_passphrase: &Option<String>) -> Result<()> {
    println!("Initializing Enigma in {}", base_dir.display());

    // Create base directory
    std::fs::create_dir_all(base_dir)?;

    // Create default config
    let config_path = EnigmaConfig::default_path(base_dir);
    if config_path.exists() {
        println!("Config already exists at {}", config_path.display());
    } else {
        let config = EnigmaConfig::default_config(base_dir);
        config.save(&config_path)?;
        println!("Created config: {}", config_path.display());
    }

    // Load config to get DB path
    let config = EnigmaConfig::load(&config_path)?;

    // Initialize SQLite database
    let db_path = Path::new(&config.enigma.db_path);
    let _db = ManifestDb::open(db_path)?;
    println!("Initialized database: {}", db_path.display());

    // Initialize key provider
    let keyfile_path = Path::new(&config.enigma.keyfile_path);
    if keyfile_path.exists() {
        println!("Keyfile already exists: {}", keyfile_path.display());
    } else {
        let passphrase = crate::get_passphrase(cli_passphrase)?;
        LocalKeyProvider::create(keyfile_path, passphrase.as_bytes())?;
        println!("Created keyfile: {}", keyfile_path.display());
    }

    println!("\nEnigma initialized. Next steps:");
    println!("  1. Add cloud providers to {}", config_path.display());
    println!("  2. Run `enigma backup <path>` to create your first backup");

    Ok(())
}
