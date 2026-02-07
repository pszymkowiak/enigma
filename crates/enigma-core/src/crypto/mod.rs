use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;
use rand::rngs::OsRng;

use crate::error::{EnigmaError, Result};
use crate::types::{ChunkHash, EncryptedChunk, KeyMaterial};

/// Encrypt a raw chunk with AES-256-GCM.
///
/// - Nonce: 12 random bytes from OsRng
/// - AAD: the chunk's SHA-256 hash (anti-substitution binding)
pub fn encrypt_chunk(
    data: &[u8],
    chunk_hash: &ChunkHash,
    key: &KeyMaterial,
) -> Result<EncryptedChunk> {
    let cipher = Aes256Gcm::new_from_slice(&key.key)
        .map_err(|e| EnigmaError::Encryption(format!("Invalid key: {e}")))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: data,
                aad: chunk_hash.as_bytes(),
            },
        )
        .map_err(|e| EnigmaError::Encryption(format!("Encryption failed: {e}")))?;

    Ok(EncryptedChunk {
        hash: chunk_hash.clone(),
        nonce: nonce_bytes,
        ciphertext,
        key_id: key.id.clone(),
    })
}

/// Decrypt an encrypted chunk with AES-256-GCM, verifying AAD.
pub fn decrypt_chunk(encrypted: &EncryptedChunk, key: &KeyMaterial) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(&key.key)
        .map_err(|e| EnigmaError::Decryption(format!("Invalid key: {e}")))?;

    let nonce = Nonce::from_slice(&encrypted.nonce);

    cipher
        .decrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: &encrypted.ciphertext,
                aad: encrypted.hash.as_bytes(),
            },
        )
        .map_err(|e| EnigmaError::Decryption(format!("Decryption failed: {e}")))
}

/// Encrypt arbitrary data (for manifest, keyfiles, etc.) with a given key and AAD.
pub fn encrypt_data(data: &[u8], key: &[u8; 32], aad: &[u8]) -> Result<(Vec<u8>, [u8; 12])> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| EnigmaError::Encryption(format!("Invalid key: {e}")))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, aes_gcm::aead::Payload { msg: data, aad })
        .map_err(|e| EnigmaError::Encryption(format!("Encryption failed: {e}")))?;

    Ok((ciphertext, nonce_bytes))
}

/// Decrypt arbitrary data.
pub fn decrypt_data(
    ciphertext: &[u8],
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| EnigmaError::Decryption(format!("Invalid key: {e}")))?;

    let nonce = Nonce::from_slice(nonce);

    cipher
        .decrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|e| EnigmaError::Decryption(format!("Decryption failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> KeyMaterial {
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);
        KeyMaterial {
            id: "test-key-1".to_string(),
            key: key_bytes,
        }
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = b"Hello, Enigma! This is secret data.";
        let hash = ChunkHash([0x42; 32]);

        let encrypted = encrypt_chunk(plaintext, &hash, &key).unwrap();
        assert_ne!(encrypted.ciphertext, plaintext);
        assert_eq!(encrypted.key_id, "test-key-1");

        let decrypted = decrypt_chunk(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_key_fails_decrypt() {
        let key1 = test_key();
        let key2 = test_key();
        let plaintext = b"Secret data";
        let hash = ChunkHash([0x42; 32]);

        let encrypted = encrypt_chunk(plaintext, &hash, &key1).unwrap();
        let result = decrypt_chunk(&encrypted, &key2);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_aad_fails_decrypt() {
        let key = test_key();
        let plaintext = b"Secret data";
        let hash1 = ChunkHash([0x42; 32]);
        let hash2 = ChunkHash([0x43; 32]);

        let mut encrypted = encrypt_chunk(plaintext, &hash1, &key).unwrap();
        encrypted.hash = hash2; // tamper with AAD
        let result = decrypt_chunk(&encrypted, &key);
        assert!(result.is_err());
    }

    #[test]
    fn unique_nonces() {
        let key = test_key();
        let plaintext = b"Same data";
        let hash = ChunkHash([0x42; 32]);

        let e1 = encrypt_chunk(plaintext, &hash, &key).unwrap();
        let e2 = encrypt_chunk(plaintext, &hash, &key).unwrap();

        // Nonces must be different
        assert_ne!(e1.nonce, e2.nonce);
        // Ciphertexts must be different (due to different nonces)
        assert_ne!(e1.ciphertext, e2.ciphertext);
    }

    #[test]
    fn encrypt_decrypt_data_roundtrip() {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        let plaintext = b"manifest data";
        let aad = b"enigma-manifest";

        let (ciphertext, nonce) = encrypt_data(plaintext, &key, aad).unwrap();
        let decrypted = decrypt_data(&ciphertext, &key, &nonce, aad).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
