use anyhow::Result;
use std::path::Path;

use enigma_core::config::EnigmaConfig;
use enigma_core::config::credentials::encrypt_credential;

pub async fn run(value: &str, base_dir: &Path, cli_passphrase: &Option<String>) -> Result<()> {
    let config_path = EnigmaConfig::default_path(base_dir);
    let config = EnigmaConfig::load(&config_path)?;

    let passphrase = if config.enigma.key_provider == "local" {
        Some(crate::get_passphrase(cli_passphrase)?)
    } else {
        None
    };
    let key_provider = enigma_keys::factory::create_key_provider(
        &config.enigma.key_provider,
        passphrase.as_deref().map(|s| s.as_bytes()),
        &config.enigma.keyfile_path,
        config.enigma.vault_url.as_deref(),
        config.enigma.gcp_project_id.as_deref(),
        config.enigma.aws_region.as_deref(),
        config.enigma.secret_prefix.as_deref(),
    )
    .await?;

    let managed_key = key_provider.get_current_key().await?;
    let encrypted = encrypt_credential(value, &managed_key.key)?;

    println!("{encrypted}");
    println!("\nPaste this value in your TOML config (access_key or secret_key field).");

    Ok(())
}
