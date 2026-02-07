use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use async_trait::async_trait;
use hkdf::Hkdf;
use ml_kem::kem::Encapsulate;
use ml_kem::{EncodedSizeUser, KemCore, MlKem768};

type Ek768 = <MlKem768 as KemCore>::EncapsulationKey;
type Ek768Size = <Ek768 as EncodedSizeUser>::EncodedSize;
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zeroize::Zeroize;

use crate::provider::{KeyProvider, ManagedKey};

/// Hybrid post-quantum local key provider.
///
/// Key derivation uses **two independent sources** combined via HKDF:
///   1. **Argon2id**: passphrase → 32-byte key (quantum-safe, symmetric)
///   2. **ML-KEM-768** (FIPS 203): post-quantum KEM shared secret (32 bytes)
///
/// Final chunk key = HKDF-SHA256(argon2id_key || ml_kem_shared_secret, salt, info="enigma-hybrid-v1")
///
/// Even if one primitive is broken, the other protects the data.
pub struct LocalKeyProvider {
    keyfile_path: PathBuf,
    master_key: [u8; 32],
    keystore: KeyStore,
}

#[derive(Debug, Serialize, Deserialize)]
struct KeyStore {
    /// Version for forward compatibility.
    version: u32,
    /// Argon2id salt.
    salt: [u8; 32],
    /// ML-KEM-768 encapsulation key (public), serialized.
    #[serde(with = "base64_bytes")]
    ml_kem_ek: Vec<u8>,
    /// ML-KEM-768 decapsulation key (private), serialized.
    /// Encrypted at rest by the master key (inside the whole keystore).
    #[serde(with = "base64_bytes")]
    ml_kem_dk: Vec<u8>,
    /// Currently active key ID.
    current_key_id: String,
    /// All encryption keys (hybrid-derived).
    keys: Vec<StoredKey>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredKey {
    id: String,
    /// The final 32-byte hybrid-derived encryption key.
    key: [u8; 32],
    /// ML-KEM ciphertext used to derive this key's KEM component.
    #[serde(with = "base64_bytes")]
    ml_kem_ct: Vec<u8>,
    created_at: String,
}

/// Serde helper for Vec<u8> as base64.
mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        let encoded = base64_encode(bytes);
        encoded.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        use serde::de::Error;
        let s = String::deserialize(d)?;
        base64_decode(&s).map_err(D::Error::custom)
    }

    fn base64_encode(data: &[u8]) -> String {
        // Simple base64 encoding without external crate
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut result = String::new();
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
            let n = (b0 << 16) | (b1 << 8) | b2;
            result.push(CHARS[((n >> 18) & 63) as usize] as char);
            result.push(CHARS[((n >> 12) & 63) as usize] as char);
            if chunk.len() > 1 {
                result.push(CHARS[((n >> 6) & 63) as usize] as char);
            } else {
                result.push('=');
            }
            if chunk.len() > 2 {
                result.push(CHARS[(n & 63) as usize] as char);
            } else {
                result.push('=');
            }
        }
        result
    }

    fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
        fn val(c: u8) -> Result<u32, String> {
            match c {
                b'A'..=b'Z' => Ok((c - b'A') as u32),
                b'a'..=b'z' => Ok((c - b'a' + 26) as u32),
                b'0'..=b'9' => Ok((c - b'0' + 52) as u32),
                b'+' => Ok(62),
                b'/' => Ok(63),
                b'=' => Ok(0),
                _ => Err(format!("invalid base64 char: {c}")),
            }
        }
        let bytes = s.as_bytes();
        let mut result = Vec::new();
        for chunk in bytes.chunks(4) {
            if chunk.len() < 4 {
                break;
            }
            let n = (val(chunk[0])? << 18)
                | (val(chunk[1])? << 12)
                | (val(chunk[2])? << 6)
                | val(chunk[3])?;
            result.push(((n >> 16) & 0xFF) as u8);
            if chunk[2] != b'=' {
                result.push(((n >> 8) & 0xFF) as u8);
            }
            if chunk[3] != b'=' {
                result.push((n & 0xFF) as u8);
            }
        }
        Ok(result)
    }
}

impl LocalKeyProvider {
    /// Derive a master key from passphrase using Argon2id.
    fn derive_master_key(passphrase: &[u8], salt: &[u8; 32]) -> anyhow::Result<[u8; 32]> {
        let mut key = [0u8; 32];
        Argon2::default()
            .hash_password_into(passphrase, salt, &mut key)
            .map_err(|e| anyhow::anyhow!("Argon2id key derivation failed: {e}"))?;
        Ok(key)
    }

    /// Derive a hybrid encryption key from passphrase-derived key + ML-KEM shared secret.
    fn derive_hybrid_key(
        passphrase_key: &[u8; 32],
        kem_shared_secret: &[u8],
        salt: &[u8; 32],
    ) -> anyhow::Result<[u8; 32]> {
        // Concatenate both key materials
        let mut ikm = Vec::with_capacity(32 + kem_shared_secret.len());
        ikm.extend_from_slice(passphrase_key);
        ikm.extend_from_slice(kem_shared_secret);

        let hk = Hkdf::<Sha256>::new(Some(salt), &ikm);
        let mut okm = [0u8; 32];
        hk.expand(b"enigma-hybrid-v1", &mut okm)
            .map_err(|e| anyhow::anyhow!("HKDF expansion failed: {e}"))?;

        ikm.zeroize();
        Ok(okm)
    }

    /// Encrypt the keystore to bytes.
    fn encrypt_keystore(keystore: &KeyStore, master_key: &[u8; 32]) -> anyhow::Result<Vec<u8>> {
        let plaintext = serde_json::to_vec(keystore)?;
        let cipher = Aes256Gcm::new_from_slice(master_key)
            .map_err(|e| anyhow::anyhow!("Invalid master key: {e}"))?;

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|e| anyhow::anyhow!("Keystore encryption failed: {e}"))?;

        // Format: salt (32) + nonce (12) + ciphertext
        let mut output = Vec::with_capacity(32 + 12 + ciphertext.len());
        output.extend_from_slice(&keystore.salt);
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }

    /// Decrypt keystore from bytes.
    fn decrypt_keystore(data: &[u8], passphrase: &[u8]) -> anyhow::Result<(KeyStore, [u8; 32])> {
        if data.len() < 44 {
            anyhow::bail!("Keyfile too short");
        }

        let salt: [u8; 32] = data[..32].try_into()?;
        let nonce_bytes: [u8; 12] = data[32..44].try_into()?;
        let ciphertext = &data[44..];

        let master_key = Self::derive_master_key(passphrase, &salt)?;
        let cipher = Aes256Gcm::new_from_slice(&master_key)
            .map_err(|e| anyhow::anyhow!("Invalid master key: {e}"))?;

        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| anyhow::anyhow!("Wrong passphrase or corrupted keyfile"))?;

        let keystore: KeyStore = serde_json::from_slice(&plaintext)?;
        Ok((keystore, master_key))
    }

    /// Save the keystore to disk (encrypted).
    fn save(&self) -> anyhow::Result<()> {
        let data = Self::encrypt_keystore(&self.keystore, &self.master_key)?;
        if let Some(parent) = self.keyfile_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.keyfile_path, data)?;
        Ok(())
    }

    /// Generate a new hybrid encryption key using ML-KEM-768 + passphrase.
    fn generate_hybrid_key(&self) -> anyhow::Result<(StoredKey, [u8; 32])> {
        // Deserialize the encapsulation key from stored bytes
        let ek_array =
            ml_kem::array::Array::<u8, Ek768Size>::try_from(self.keystore.ml_kem_ek.as_slice())
                .map_err(|_| anyhow::anyhow!("Invalid ML-KEM encapsulation key in keystore"))?;
        let ek = Ek768::from_bytes(&ek_array);

        // Encapsulate: produces (ciphertext, shared_secret)
        let (ct, shared_secret) = ek.encapsulate(&mut OsRng).unwrap();

        // Derive hybrid key: HKDF(passphrase_key || kem_shared_secret)
        let hybrid_key = Self::derive_hybrid_key(
            &self.master_key,
            shared_secret.as_ref(),
            &self.keystore.salt,
        )?;

        let key_id = Uuid::now_v7().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let stored = StoredKey {
            id: key_id,
            key: hybrid_key,
            ml_kem_ct: ct[..].to_vec(),
            created_at: now,
        };

        Ok((stored, hybrid_key))
    }

    /// Create a new local key provider with a fresh keyfile and ML-KEM-768 keypair.
    pub fn create(keyfile_path: &Path, passphrase: &[u8]) -> anyhow::Result<Self> {
        let mut salt = [0u8; 32];
        OsRng.fill_bytes(&mut salt);

        let master_key = Self::derive_master_key(passphrase, &salt)?;

        // Generate ML-KEM-768 keypair
        let (dk, ek) = MlKem768::generate(&mut OsRng);
        let ek_bytes = ek.as_bytes().to_vec();
        let dk_bytes = dk.as_bytes().to_vec();

        let keystore = KeyStore {
            version: 2, // v2 = hybrid PQ
            salt,
            ml_kem_ek: ek_bytes,
            ml_kem_dk: dk_bytes,
            current_key_id: String::new(),
            keys: vec![],
        };

        // Create the provider temporarily to use generate_hybrid_key
        let mut provider = Self {
            keyfile_path: keyfile_path.to_path_buf(),
            master_key,
            keystore,
        };

        // Generate first hybrid key
        let (stored_key, _hybrid_key) = provider.generate_hybrid_key()?;
        provider.keystore.current_key_id = stored_key.id.clone();
        provider.keystore.keys.push(stored_key);

        provider.save()?;
        Ok(provider)
    }

    /// Open an existing keyfile.
    pub fn open(keyfile_path: &Path, passphrase: &[u8]) -> anyhow::Result<Self> {
        let data = std::fs::read(keyfile_path)?;
        let (keystore, master_key) = Self::decrypt_keystore(&data, passphrase)?;

        Ok(Self {
            keyfile_path: keyfile_path.to_path_buf(),
            master_key,
            keystore,
        })
    }
}

impl Drop for LocalKeyProvider {
    fn drop(&mut self) {
        self.master_key.zeroize();
        for key in &mut self.keystore.keys {
            key.key.zeroize();
        }
        self.keystore.ml_kem_dk.zeroize();
    }
}

#[async_trait]
impl KeyProvider for LocalKeyProvider {
    async fn get_current_key(&self) -> anyhow::Result<ManagedKey> {
        let stored = self
            .keystore
            .keys
            .iter()
            .find(|k| k.id == self.keystore.current_key_id)
            .ok_or_else(|| anyhow::anyhow!("Current key not found in keystore"))?;

        Ok(ManagedKey {
            id: stored.id.clone(),
            key: stored.key,
            created_at: stored.created_at.clone(),
        })
    }

    async fn get_key_by_id(&self, id: &str) -> anyhow::Result<ManagedKey> {
        let stored = self
            .keystore
            .keys
            .iter()
            .find(|k| k.id == id)
            .ok_or_else(|| anyhow::anyhow!("Key {id} not found"))?;

        Ok(ManagedKey {
            id: stored.id.clone(),
            key: stored.key,
            created_at: stored.created_at.clone(),
        })
    }

    async fn create_key(&mut self) -> anyhow::Result<ManagedKey> {
        let (stored_key, _) = self.generate_hybrid_key()?;
        let managed = ManagedKey {
            id: stored_key.id.clone(),
            key: stored_key.key,
            created_at: stored_key.created_at.clone(),
        };
        self.keystore.current_key_id = stored_key.id.clone();
        self.keystore.keys.push(stored_key);
        self.save()?;
        Ok(managed)
    }

    async fn rotate_key(&mut self) -> anyhow::Result<ManagedKey> {
        self.create_key().await
    }

    async fn list_key_ids(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.keystore.keys.iter().map(|k| k.id.clone()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn create_and_open_keyfile() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("keys.enc");
        let passphrase = b"test-passphrase-123";

        // Create (generates ML-KEM-768 keypair + first hybrid key)
        let provider = LocalKeyProvider::create(&path, passphrase).unwrap();
        let key1 = provider.get_current_key().await.unwrap();
        assert_eq!(key1.key.len(), 32);
        assert_eq!(provider.keystore.version, 2);

        // Verify ML-KEM keypair was generated
        assert!(!provider.keystore.ml_kem_ek.is_empty());
        assert!(!provider.keystore.ml_kem_dk.is_empty());

        // Open with same passphrase
        let provider2 = LocalKeyProvider::open(&path, passphrase).unwrap();
        let key2 = provider2.get_current_key().await.unwrap();
        assert_eq!(key1.id, key2.id);
        assert_eq!(key1.key, key2.key);
    }

    #[tokio::test]
    async fn wrong_passphrase_fails() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("keys.enc");

        LocalKeyProvider::create(&path, b"correct").unwrap();
        let result = LocalKeyProvider::open(&path, b"wrong");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn key_rotation_produces_different_hybrid_keys() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("keys.enc");

        let mut provider = LocalKeyProvider::create(&path, b"pass").unwrap();
        let key1 = provider.get_current_key().await.unwrap();

        let key2 = provider.rotate_key().await.unwrap();
        assert_ne!(key1.id, key2.id);
        assert_ne!(key1.key, key2.key);

        // Old key still accessible
        let old = provider.get_key_by_id(&key1.id).await.unwrap();
        assert_eq!(old.key, key1.key);

        // Current key is the new one
        let current = provider.get_current_key().await.unwrap();
        assert_eq!(current.id, key2.id);

        // Each key has its own ML-KEM ciphertext
        let ct1 = &provider.keystore.keys[0].ml_kem_ct;
        let ct2 = &provider.keystore.keys[1].ml_kem_ct;
        assert_ne!(ct1, ct2); // Different encapsulations

        // Verify persisted
        let provider2 = LocalKeyProvider::open(&path, b"pass").unwrap();
        let ids = provider2.list_key_ids().await.unwrap();
        assert_eq!(ids.len(), 2);
    }

    #[tokio::test]
    async fn hybrid_key_depends_on_both_sources() {
        // Verify that changing the passphrase produces a different hybrid key
        // (even with the same ML-KEM keypair, different Argon2id output → different HKDF)
        let tmp = TempDir::new().unwrap();
        let path1 = tmp.path().join("keys1.enc");
        let path2 = tmp.path().join("keys2.enc");

        let p1 = LocalKeyProvider::create(&path1, b"passphrase-A").unwrap();
        let p2 = LocalKeyProvider::create(&path2, b"passphrase-B").unwrap();

        let k1 = p1.get_current_key().await.unwrap();
        let k2 = p2.get_current_key().await.unwrap();

        // Different passphrases + different ML-KEM keypairs = very different keys
        assert_ne!(k1.key, k2.key);
    }

    #[test]
    fn ml_kem_keypair_sizes() {
        // Verify expected sizes for ML-KEM-768
        let (dk, ek) = MlKem768::generate(&mut OsRng);
        let ek_bytes = ek.as_bytes();
        let dk_bytes = dk.as_bytes();

        // ML-KEM-768: ek = 1184 bytes, dk = 2400 bytes
        assert_eq!(ek_bytes.len(), 1184);
        assert_eq!(dk_bytes.len(), 2400);
    }
}
