pub mod auth;
pub mod get;
pub mod list;
pub mod multipart;
pub mod put;
pub mod service;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use enigma_core::config::EnigmaConfig;
use enigma_core::distributor::Distributor;
use enigma_core::manifest::ManifestDb;
use enigma_core::types::KeyMaterial;
use enigma_storage::provider::StorageProvider;

/// Shared state for the Enigma S3 service.
pub struct EnigmaS3State {
    pub db: Mutex<ManifestDb>,
    pub providers: HashMap<i64, Box<dyn StorageProvider>>,
    pub distributor: Distributor,
    pub key_material: KeyMaterial,
    pub config: EnigmaConfig,
}

pub type SharedState = Arc<EnigmaS3State>;
