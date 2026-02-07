mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "enigma")]
#[command(about = "Multi-cloud encrypted backup tool")]
#[command(version)]
struct Cli {
    /// Path to the Enigma config directory (default: ~/.enigma)
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,

    /// Passphrase for key encryption (or set ENIGMA_PASSPHRASE env var).
    /// If not provided, will prompt interactively.
    #[arg(long, global = true, env = "ENIGMA_PASSPHRASE")]
    passphrase: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize Enigma configuration and keyfile
    Init,

    /// Backup a directory
    Backup {
        /// Path to the directory or file to backup
        path: PathBuf,
    },

    /// Restore a backup
    Restore {
        /// Backup ID to restore
        backup_id: String,
        /// Destination directory
        dest: PathBuf,
        /// Only restore files matching this path prefix
        #[arg(long)]
        path: Option<String>,
        /// Only restore files matching this glob pattern
        #[arg(long)]
        glob: Option<String>,
        /// List matching files without restoring
        #[arg(long)]
        list: bool,
    },

    /// List all backups
    List,

    /// Show status of the latest backup
    Status,

    /// Verify integrity of a backup
    Verify {
        /// Backup ID to verify
        backup_id: String,
    },

    /// Show current configuration
    Config,

    /// Garbage collect orphaned chunks
    Gc {
        /// List orphans without deleting
        #[arg(long)]
        dry_run: bool,
    },

    /// Encrypt a credential value for use in TOML config
    EncryptCred {
        /// The plaintext value to encrypt
        value: String,
    },
}

/// Get passphrase from CLI arg, env var, or interactive prompt.
pub fn get_passphrase(cli_passphrase: &Option<String>) -> anyhow::Result<String> {
    if let Some(p) = cli_passphrase {
        return Ok(p.clone());
    }
    // Interactive prompt
    use std::io::{self, Write};
    print!("Enter passphrase: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("enigma=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    let base_dir = match cli.config_dir {
        Some(ref dir) => dir.clone(),
        None => enigma_core::config::EnigmaConfig::default_base_dir()?,
    };

    let rt = tokio::runtime::Runtime::new()?;

    match cli.command {
        Commands::Init => rt.block_on(commands::init::run(&base_dir, &cli.passphrase)),
        Commands::Backup { ref path } => {
            rt.block_on(commands::backup::run(path, &base_dir, &cli.passphrase))
        }
        Commands::Restore {
            ref backup_id,
            ref dest,
            ref path,
            ref glob,
            list,
        } => rt.block_on(commands::restore::run(
            backup_id,
            dest,
            &base_dir,
            &cli.passphrase,
            path.as_deref(),
            glob.as_deref(),
            list,
        )),
        Commands::List => commands::list::run(&base_dir),
        Commands::Status => commands::status::run(&base_dir),
        Commands::Verify { ref backup_id } => {
            rt.block_on(commands::verify::run(backup_id, &base_dir, &cli.passphrase))
        }
        Commands::Config => commands::config::run(&base_dir),
        Commands::Gc { dry_run } => rt.block_on(commands::gc::run(&base_dir, dry_run)),
        Commands::EncryptCred { ref value } => rt.block_on(commands::encrypt_cred::run(
            value,
            &base_dir,
            &cli.passphrase,
        )),
    }
}
